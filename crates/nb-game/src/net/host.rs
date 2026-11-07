//! Hôte d'une partie LAN : serveur intégré au jeu du joueur qui héberge.
//!
//! À chaque tick, [`HostNet::pre_tick`] traite les messages reçus (arrivées, positions, blocs,
//! coups, chat) avant la simulation ; [`HostNet::post_tick`] diffuse ensuite le résultat (blocs
//! modifiés, chunks, instantanés des joueurs et des mobs, santé, heure).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::DVec3;
use nb_core::ChunkPos;
use nb_ecs::player::{EYE_OFFSET, PLAYER_MAX_HEALTH, PLAYER_WIDTH};
use nb_ecs::{
    Collider, EntityId, GameMode, Health, MobType, Player, RemotePlayer, Transform, Velocity,
};
use nb_gfx::SkyClock;
use nb_net::{
    local_ipv4, unique_name, Beacon, ClientMsg, ConnId, MobPose, NetServer, PlayerPose,
    ServerEvent, ServerInfo, ServerMsg, Welcome, MAX_PLAYERS, PROTOCOL_VERSION,
};
use nb_world::storage::encode_chunk;

use super::{
    chunk_dist, chunk_of, ring_offsets, Avatar, Known, PlayerEntry, Smooth, HANDSHAKE_SECS,
    MAX_CHAT_LEN, MAX_REACH, MOB_SYNC_RADIUS, SNAPSHOT_INTERVAL, TIMEOUT_SECS,
};
use crate::session::{Registries, WorldSession};

/// Identifiant réseau de l'hôte.
pub const HOST_ID: u16 = 0;
/// Chunks envoyés au plus par client et par passe (toutes les 100 ms).
const CHUNKS_PER_PASS: usize = 6;
const SYNC_INTERVAL: f32 = 0.1;
const TIME_INTERVAL: f32 = 2.0;
/// Pause entre deux coups d'un même joueur.
const ATTACK_COOLDOWN: Duration = Duration::from_millis(150);
/// Pendant ce délai après des dégâts, la santé annoncée par le client est ignorée (le message
/// `SetHealth` est encore en route).
const HEALTH_HOLD: Duration = Duration::from_millis(600);

/// Réglages de démarrage.
#[derive(Clone, Debug)]
pub struct HostOptions {
    /// Port TCP (`0` = libre, utile aux tests).
    pub port: u16,
    /// Port UDP de découverte (`None` = port par défaut).
    pub discovery_port: Option<u16>,
    pub host_name: String,
    pub world_name: String,
}

/// Ce que l'hôte emprunte au jeu pendant un tick.
pub struct HostCtx<'a> {
    pub session: &'a mut WorldSession,
    pub regs: &'a Registries,
    pub sky: &'a SkyClock,
    /// Pieds, lacet et tangage (degrés) du joueur local.
    pub host_pose: (DVec3, f32, f32),
    /// Lignes à ajouter au chat de l'hôte : (texte, message système ?).
    pub chat: &'a mut Vec<(String, bool)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Handshake,
    Playing,
}

struct Conn {
    addr: SocketAddr,
    phase: Phase,
    connected_at: Instant,
    last_seen: Instant,
    last_attack: Instant,
    player_id: u16,
    name: String,
    entity: Option<EntityId>,
    render_distance: i32,
    pos: DVec3,
    yaw: f32,
    pitch: f32,
    creative: bool,
    /// Dernière santé connue des deux côtés.
    synced_health: f32,
    health_hold_until: Instant,
    known: Known,
    shown: Smooth,
}

impl Conn {
    fn new(addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            addr,
            phase: Phase::Handshake,
            connected_at: now,
            last_seen: now,
            last_attack: now,
            player_id: 0,
            name: String::new(),
            entity: None,
            render_distance: 6,
            pos: DVec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            creative: false,
            synced_health: PLAYER_MAX_HEALTH,
            health_hold_until: now,
            known: HashSet::new(),
            shown: Smooth::new(DVec3::ZERO, 0.0, 0.0),
        }
    }

    fn playing(&self) -> bool {
        self.phase == Phase::Playing
    }

    fn eye(&self) -> DVec3 {
        self.pos + DVec3::new(0.0, f64::from(EYE_OFFSET), 0.0)
    }
}

/// Serveur de la partie.
pub struct HostNet {
    server: NetServer,
    beacon: Option<Beacon>,
    host_name: String,
    world_name: String,
    mob_types: Vec<String>,
    mob_index: HashMap<String, u16>,
    valid_blocks: Box<[bool; 256]>,
    conns: BTreeMap<ConnId, Conn>,
    next_player_id: u16,
    snapshot_timer: f32,
    sync_timer: f32,
    time_timer: f32,
    ring: Vec<(i32, i32)>,
    /// Message d'erreur si la découverte UDP n'a pas pu démarrer (la partie reste jouable par
    /// adresse).
    pub discovery_problem: Option<String>,
}

impl std::fmt::Debug for HostNet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostNet")
            .field("port", &self.server.port())
            .field("players", &self.player_count())
            .finish()
    }
}

impl HostNet {
    pub fn start(opts: HostOptions, regs: &Registries) -> std::io::Result<Self> {
        let server = NetServer::bind(opts.port)?;
        let mob_types: Vec<String> = regs.mobs.all().iter().map(|m| m.id.clone()).collect();
        let mob_index = mob_types
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i as u16))
            .collect();
        let mut valid = Box::new([false; 256]);
        for b in regs.blocks.iter() {
            valid[usize::from(b.id)] = true;
        }
        let mut net = Self {
            server,
            beacon: None,
            host_name: opts.host_name,
            world_name: opts.world_name,
            mob_types,
            mob_index,
            valid_blocks: valid,
            conns: BTreeMap::new(),
            next_player_id: 1,
            snapshot_timer: 0.0,
            sync_timer: 0.0,
            time_timer: 0.0,
            ring: ring_offsets(17),
            discovery_problem: None,
        };
        let info = net.server_info();
        let dport = opts
            .discovery_port
            .unwrap_or(nb_net::DEFAULT_DISCOVERY_PORT);
        match Beacon::start(dport, info) {
            Ok(b) => net.beacon = Some(b),
            Err(e) => {
                net.discovery_problem = Some(format!(
                    "Friends will not see this game in their list ({e}); they can still type the address"
                ));
            }
        }
        Ok(net)
    }

    pub fn port(&self) -> u16 {
        self.server.port()
    }

    fn player_count(&self) -> usize {
        1 + self.conns.values().filter(|c| c.playing()).count()
    }

    fn server_info(&self) -> ServerInfo {
        ServerInfo {
            protocol: PROTOCOL_VERSION,
            port: self.server.port(),
            host_name: self.host_name.clone(),
            world_name: self.world_name.clone(),
            players: self.player_count().min(255) as u8,
            max_players: MAX_PLAYERS as u8,
        }
    }

    /// Adresses à donner aux amis.
    pub fn addresses(&self) -> Vec<String> {
        let port = self.server.port();
        match local_ipv4() {
            Some(ip) => vec![format!("{ip}:{port}")],
            None => vec![format!("127.0.0.1:{port}")],
        }
    }

    /// Joueurs connectés, l'hôte en tête.
    pub fn players(&self) -> Vec<PlayerEntry> {
        let mut v = vec![PlayerEntry {
            name: self.host_name.clone(),
            is_self: false,
            is_host: true,
            ping_ms: None,
        }];
        v.extend(
            self.conns
                .values()
                .filter(|c| c.playing())
                .map(|c| PlayerEntry {
                    name: c.name.clone(),
                    is_self: false,
                    is_host: false,
                    ping_ms: None,
                }),
        );
        v
    }

    /// Joueurs distants, tels que l'hôte les dessine.
    pub fn avatars(&self) -> Vec<Avatar> {
        self.conns
            .values()
            .filter(|c| c.playing())
            .map(|c| Avatar {
                id: c.player_id,
                name: c.name.clone(),
                pos: c.shown.pos,
                yaw_deg: c.shown.yaw,
                pitch_deg: c.shown.pitch,
            })
            .collect()
    }

    fn broadcast(&self, msg: &ServerMsg, except: Option<ConnId>) {
        let frame = Arc::new(msg.encode());
        for (&id, c) in &self.conns {
            if c.playing() && Some(id) != except {
                self.server.send_encoded(id, frame.clone());
            }
        }
    }

    fn system_chat(&self, cx: &mut HostCtx<'_>, text: String, except: Option<ConnId>) {
        self.broadcast(
            &ServerMsg::Chat {
                from: String::new(),
                text: text.clone(),
            },
            except,
        );
        cx.chat.push((text, true));
    }

    // ── Avant la simulation ─────────────────────────────────────────────────────────────

    pub fn pre_tick(&mut self, cx: &mut HostCtx<'_>, dt: f32) {
        for ev in self.server.poll() {
            match ev {
                ServerEvent::Connected { conn, addr } => {
                    self.conns.insert(conn, Conn::new(addr));
                }
                ServerEvent::Message { conn, msg } => self.on_message(cx, conn, msg),
                ServerEvent::Disconnected { conn, reason } => {
                    self.remove_conn(cx, conn, &reason, false)
                }
            }
        }
        // Délais.
        let now = Instant::now();
        let stale: Vec<(ConnId, &'static str)> = self
            .conns
            .iter()
            .filter_map(|(&id, c)| match c.phase {
                Phase::Handshake
                    if c.connected_at.elapsed() > Duration::from_secs(HANDSHAKE_SECS) =>
                {
                    Some((id, "no greeting received"))
                }
                Phase::Playing
                    if now.duration_since(c.last_seen) > Duration::from_secs(TIMEOUT_SECS) =>
                {
                    Some((id, "connection timed out"))
                }
                _ => None,
            })
            .collect();
        for (id, why) in stale {
            self.server.send(id, &ServerMsg::Kick(why.to_owned()));
            self.remove_conn(cx, id, why, true);
        }
        // Lissage de l'affichage et zones à garder chargées autour des joueurs distants.
        for c in self.conns.values_mut().filter(|c| c.playing()) {
            c.shown.step(dt);
        }
        let centers = self
            .conns
            .values()
            .filter(|c| c.playing())
            .map(|c| {
                (
                    nb_core::glam::Vec3::new(c.pos.x as f32, c.pos.y as f32, c.pos.z as f32),
                    c.render_distance,
                )
            })
            .collect();
        cx.session.world.set_extra_centers(centers);
        cx.session.sim.entities.extra_origins = self
            .conns
            .values()
            .filter(|c| c.playing())
            .map(|c| {
                let e = c.eye();
                nb_core::glam::Vec3::new(e.x as f32, e.y as f32, e.z as f32)
            })
            .collect();
    }

    fn on_message(&mut self, cx: &mut HostCtx<'_>, id: ConnId, msg: ClientMsg) {
        let Some(conn) = self.conns.get_mut(&id) else {
            return;
        };
        conn.last_seen = Instant::now();
        match msg {
            ClientMsg::Hello {
                version,
                name,
                render_distance,
            } => self.on_hello(cx, id, version, &name, render_distance),
            ClientMsg::Ping(n) => self.server.send(id, &ServerMsg::Pong(n)),
            ClientMsg::Bye => self.remove_conn(cx, id, "left the game", true),
            _ if !conn.playing() => {} // rien avant « Hello »
            ClientMsg::State {
                pos,
                yaw,
                pitch,
                health,
                creative,
            } => self.on_state(cx, id, pos, yaw, pitch, health, creative),
            ClientMsg::BlockEdit { pos, block } => self.on_block_edit(cx, id, pos, block),
            ClientMsg::Attack { entity } => self.on_attack(cx, id, entity),
            ClientMsg::Chat(text) => self.on_chat(cx, id, &text),
        }
    }

    fn on_hello(&mut self, cx: &mut HostCtx<'_>, id: ConnId, version: u16, name: &str, rd: u8) {
        if self.conns.get(&id).is_none_or(|c| c.playing()) {
            return;
        }
        let refuse = |this: &mut Self, why: String| {
            this.server.send(id, &ServerMsg::Refuse(why));
            this.server.disconnect(id);
            this.conns.remove(&id);
        };
        if version != PROTOCOL_VERSION {
            return refuse(
                self,
                format!(
                    "Incompatible versions (host {PROTOCOL_VERSION}, you {version}): both players need the same Norrblock build"
                ),
            );
        }
        if self.player_count() >= MAX_PLAYERS {
            return refuse(self, format!("The game is full ({MAX_PLAYERS} players)"));
        }
        let taken: Vec<String> = std::iter::once(self.host_name.clone())
            .chain(
                self.conns
                    .values()
                    .filter(|c| c.playing())
                    .map(|c| c.name.clone()),
            )
            .collect();
        let name = unique_name(name, taken.iter().map(String::as_str));
        let player_id = self.next_player_id;
        self.next_player_id = self.next_player_id.wrapping_add(1).max(1);

        let spawn = cx.session.world.ensure_spawn();
        let host_mode = cx
            .session
            .player()
            .and_then(|p| cx.session.sim.entities.copy::<Player>(p))
            .map_or(GameMode::Creative, |p| p.game_mode);
        let creative = host_mode == GameMode::Creative;

        // Entité du joueur distant : les mobs la voient, rien d'autre.
        let e = &mut cx.session.sim.entities;
        let eid = e.create();
        e.insert(eid, Transform::new(spawn[0], spawn[1], spawn[2]));
        e.insert(eid, Velocity::default());
        e.insert(
            eid,
            Collider::new(
                PLAYER_WIDTH,
                PLAYER_WIDTH,
                EYE_OFFSET + nb_ecs::player::HEAD_CLEARANCE,
                EYE_OFFSET,
            ),
        );
        e.insert(eid, Health::new(PLAYER_MAX_HEALTH));
        e.insert(
            eid,
            RemotePlayer {
                client: player_id,
                creative,
            },
        );

        let config_json = serde_json::to_string(&*cx.session.world.config()).unwrap_or_default();
        let welcome = Welcome {
            player_id,
            name: name.clone(),
            host_name: self.host_name.clone(),
            world_name: self.world_name.clone(),
            seed: cx.session.world.seed(),
            creative,
            allow_cheats: cx.session.allow_cheats,
            config_json,
            spawn,
            time_of_day: cx.sky.time_of_day,
            day_length: cx.sky.day_length_seconds,
            mob_types: self.mob_types.clone(),
        };
        let others: Vec<(u16, String)> = std::iter::once((HOST_ID, self.host_name.clone()))
            .chain(
                self.conns
                    .values()
                    .filter(|c| c.playing())
                    .map(|c| (c.player_id, c.name.clone())),
            )
            .collect();

        let c = self.conns.get_mut(&id).expect("connexion présente");
        c.phase = Phase::Playing;
        c.player_id = player_id;
        c.name = name.clone();
        c.entity = Some(eid);
        c.render_distance = i32::from(rd).clamp(2, 16);
        c.pos = DVec3::new(spawn[0], spawn[1], spawn[2]);
        c.creative = creative;
        c.shown = Smooth::new(c.pos, 0.0, 0.0);
        c.synced_health = PLAYER_MAX_HEALTH;

        self.server.send(id, &ServerMsg::Welcome(Box::new(welcome)));
        for (pid, pname) in others {
            self.server.send(
                id,
                &ServerMsg::PlayerJoined {
                    id: pid,
                    name: pname,
                },
            );
        }
        self.broadcast(
            &ServerMsg::PlayerJoined {
                id: player_id,
                name: name.clone(),
            },
            Some(id),
        );
        self.system_chat(cx, format!("{name} joined the game"), Some(id));
        self.refresh_beacon();
    }

    #[allow(clippy::too_many_arguments)]
    fn on_state(
        &mut self,
        cx: &mut HostCtx<'_>,
        id: ConnId,
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
        health: f32,
        creative: bool,
    ) {
        if !pos.iter().all(|v| v.is_finite()) || !yaw.is_finite() || !pitch.is_finite() {
            return;
        }
        let Some(c) = self.conns.get_mut(&id) else {
            return;
        };
        c.pos = DVec3::new(pos[0], pos[1].clamp(-64.0, 4096.0), pos[2]);
        c.yaw = yaw;
        c.pitch = pitch.clamp(-90.0, 90.0);
        c.creative = creative;
        c.shown.set_target(c.pos, c.yaw, c.pitch);
        let hold = Instant::now() < c.health_hold_until;
        let Some(eid) = c.entity else {
            return;
        };
        let e = &mut cx.session.sim.entities;
        if let Some(t) = e.get_mut::<Transform>(eid) {
            t.pos = c.pos;
        }
        if let Some(r) = e.get_mut::<RemotePlayer>(eid) {
            r.creative = creative;
        }
        if !hold && health.is_finite() {
            if let Some(h) = e.get_mut::<Health>(eid) {
                h.health = health.clamp(0.0, h.max_health);
                h.alive = h.health > 0.0;
                c.synced_health = h.health;
            }
        }
    }

    fn on_block_edit(&mut self, cx: &mut HostCtx<'_>, id: ConnId, pos: [i32; 3], block: u8) {
        let Some(c) = self.conns.get(&id) else {
            return;
        };
        let [x, y, z] = pos;
        let cm = cx.session.world.chunk_manager();
        let center = DVec3::new(f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5);
        let ok = (0..nb_core::math::CHUNK_SIZE_Y as i32).contains(&y)
            && self.valid_blocks[usize::from(block)]
            && center.distance(c.eye()) <= MAX_REACH
            && cm.chunk(chunk_of(x, z)).is_some();
        if ok {
            // Consigné par le journal : diffusé à tous (l'expéditeur compris) par `post_tick`.
            cm.set_block(x, y, z, block);
        } else {
            // Refusé : on renvoie la vérité pour corriger l'affichage du client.
            let actual = cm.block(x, y, z);
            self.server
                .send(id, &ServerMsg::BlockChange { pos, block: actual });
        }
    }

    fn on_attack(&mut self, cx: &mut HostCtx<'_>, id: ConnId, entity: u32) {
        let Some(c) = self.conns.get_mut(&id) else {
            return;
        };
        if c.last_attack.elapsed() < ATTACK_COOLDOWN {
            return;
        }
        c.last_attack = Instant::now();
        let eye = c.eye();
        let attacker = c.entity;
        let mob = EntityId(entity as i32);
        let sim = &mut cx.session.sim;
        if !sim.entities.contains(mob) || !sim.entities.has::<MobType>(mob) {
            return;
        }
        let Some(t) = sim.entities.copy::<Transform>(mob) else {
            return;
        };
        if t.pos.distance(eye) > MAX_REACH + 3.0 {
            return;
        }
        let cfg = cx.session.world.config();
        nb_ecs::health::hit_mob(
            &mut sim.entities,
            mob,
            attacker,
            &cfg,
            &cx.regs.mobs,
            &mut cx.session.rngs.passive,
        );
    }

    fn on_chat(&mut self, cx: &mut HostCtx<'_>, id: ConnId, text: &str) {
        let text: String = text
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_CHAT_LEN)
            .collect();
        let text = text.trim();
        let Some(c) = self.conns.get(&id) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let from = c.name.clone();
        self.broadcast(
            &ServerMsg::Chat {
                from: from.clone(),
                text: text.to_owned(),
            },
            None,
        );
        cx.chat.push((format!("<{from}> {text}"), false));
    }

    /// Message de chat du joueur local.
    pub fn host_chat(&mut self, text: &str) {
        let text: String = text
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_CHAT_LEN)
            .collect();
        if text.trim().is_empty() {
            return;
        }
        self.broadcast(
            &ServerMsg::Chat {
                from: self.host_name.clone(),
                text: text.trim().to_owned(),
            },
            None,
        );
    }

    fn remove_conn(&mut self, cx: &mut HostCtx<'_>, id: ConnId, reason: &str, close: bool) {
        let Some(c) = self.conns.remove(&id) else {
            return;
        };
        if close {
            self.server.disconnect(id);
        }
        if let Some(e) = c.entity {
            cx.session.sim.entities.destroy(e);
        }
        if c.playing() {
            self.broadcast(&ServerMsg::PlayerLeft { id: c.player_id }, None);
            let why = if reason.is_empty() || reason == "connexion fermée" {
                String::new()
            } else {
                format!(" ({reason})")
            };
            self.system_chat(cx, format!("{} left the game{why}", c.name), None);
            self.refresh_beacon();
        } else {
            eprintln!("réseau : {} déconnecté avant le salut ({reason})", c.addr);
        }
    }

    fn refresh_beacon(&self) {
        if let Some(b) = &self.beacon {
            b.set_info(self.server_info());
        }
    }

    // ── Après la simulation ─────────────────────────────────────────────────────────────

    pub fn post_tick(&mut self, cx: &mut HostCtx<'_>, dt: f32) {
        self.send_block_changes(cx);
        self.sync_health(cx);

        self.sync_timer += dt;
        if self.sync_timer >= SYNC_INTERVAL {
            self.sync_timer = 0.0;
            self.sync_chunks(cx);
        }
        self.snapshot_timer += dt;
        if self.snapshot_timer >= SNAPSHOT_INTERVAL {
            self.snapshot_timer = 0.0;
            self.send_snapshots(cx);
        }
        self.time_timer += dt;
        if self.time_timer >= TIME_INTERVAL {
            self.time_timer = 0.0;
            self.broadcast(
                &ServerMsg::Time {
                    time_of_day: cx.sky.time_of_day,
                    day_length: cx.sky.day_length_seconds,
                },
                None,
            );
        }
    }

    fn send_block_changes(&self, cx: &mut HostCtx<'_>) {
        let edits = cx.session.world.chunk_manager().take_edits();
        for e in edits {
            let cp = chunk_of(e.x, e.z);
            let frame = Arc::new(
                ServerMsg::BlockChange {
                    pos: [e.x, e.y, e.z],
                    block: e.id,
                }
                .encode(),
            );
            for (&id, c) in &self.conns {
                if c.playing() && c.known.contains(&cp) {
                    self.server.send_encoded(id, frame.clone());
                }
            }
        }
    }

    fn sync_health(&mut self, cx: &mut HostCtx<'_>) {
        for (&id, c) in &mut self.conns {
            let Some(eid) = c.entity else { continue };
            let Some(h) = cx.session.sim.entities.copy::<Health>(eid) else {
                continue;
            };
            if h.health <= 0.0 || !h.alive {
                // Mort : le client réapparaît chez lui ; l'entité repart pleine de vie.
                self.server.send(id, &ServerMsg::SetHealth(0.0));
                if let Some(hm) = cx.session.sim.entities.get_mut::<Health>(eid) {
                    hm.health = hm.max_health;
                    hm.alive = true;
                }
                c.synced_health = h.max_health;
                c.health_hold_until = Instant::now() + HEALTH_HOLD * 2;
            } else if h.health < c.synced_health - 0.001 {
                self.server.send(id, &ServerMsg::SetHealth(h.health));
                c.synced_health = h.health;
                c.health_hold_until = Instant::now() + HEALTH_HOLD;
            }
        }
    }

    fn sync_chunks(&mut self, cx: &mut HostCtx<'_>) {
        let cm = cx.session.world.chunk_manager();
        for (&id, c) in &mut self.conns {
            if !c.playing() {
                continue;
            }
            let center = chunk_of(c.pos.x.floor() as i32, c.pos.z.floor() as i32);
            let reach = c.render_distance + 1;
            // Chunks trop loin : oubliés (le client les décharge aussi).
            c.known.retain(|&p| chunk_dist(p, center) <= reach + 2);
            let mut budget = CHUNKS_PER_PASS;
            for &(dx, dz) in &self.ring {
                if dx.abs().max(dz.abs()) > reach {
                    continue;
                }
                let pos = ChunkPos::new(center.x + dx, center.z + dz);
                if c.known.contains(&pos) {
                    continue;
                }
                let Some(vox) = cm.voxels_of(pos) else {
                    continue; // pas encore généré chez l'hôte : au prochain passage
                };
                self.server.send(
                    id,
                    &ServerMsg::Chunk {
                        cx: pos.x,
                        cz: pos.z,
                        data: encode_chunk(&vox),
                    },
                );
                c.known.insert(pos);
                budget -= 1;
                if budget == 0 {
                    break;
                }
            }
        }
    }

    fn send_snapshots(&self, cx: &mut HostCtx<'_>) {
        if !self.conns.values().any(Conn::playing) {
            return;
        }
        let (hp, hyaw, hpitch) = cx.host_pose;
        let mut players = vec![PlayerPose {
            id: HOST_ID,
            pos: [hp.x as f32, hp.y as f32, hp.z as f32],
            yaw: hyaw,
            pitch: hpitch,
        }];
        players.extend(
            self.conns
                .values()
                .filter(|c| c.playing())
                .map(|c| PlayerPose {
                    id: c.player_id,
                    pos: [c.pos.x as f32, c.pos.y as f32, c.pos.z as f32],
                    yaw: c.yaw,
                    pitch: c.pitch,
                }),
        );
        let e = &cx.session.sim.entities;
        let mobs: Vec<(MobPose, DVec3)> = e
            .query_ids::<(&Transform, &MobType, &Health)>()
            .into_iter()
            .filter_map(|id| {
                let t = e.copy::<Transform>(id)?;
                let kind = *self.mob_index.get(&e.get::<MobType>(id)?.mob_id)?;
                let h = e.copy::<Health>(id)?;
                Some((
                    MobPose {
                        id: id.0 as u32,
                        kind,
                        pos: [t.pos.x as f32, t.pos.y as f32, t.pos.z as f32],
                        yaw: t.yaw,
                        pitch: t.pitch,
                        health: h.health,
                    },
                    t.pos,
                ))
            })
            .collect();
        for (&id, c) in &self.conns {
            if !c.playing() {
                continue;
            }
            let near: Vec<MobPose> = mobs
                .iter()
                .filter(|(_, p)| {
                    let (dx, dz) = (p.x - c.pos.x, p.z - c.pos.z);
                    dx * dx + dz * dz <= MOB_SYNC_RADIUS * MOB_SYNC_RADIUS
                })
                .map(|(m, _)| *m)
                .take(400)
                .collect();
            let others: Vec<PlayerPose> = players
                .iter()
                .filter(|p| p.id != c.player_id)
                .copied()
                .collect();
            self.server.send(
                id,
                &ServerMsg::Snapshot {
                    players: others,
                    mobs: near,
                },
            );
        }
    }

    // ── Fin de partie ───────────────────────────────────────────────────────────────────

    /// Prévient tout le monde, ferme le réseau et retire les joueurs distants du monde.
    pub fn close(&mut self, session: &mut WorldSession, reason: &str) {
        let ids: Vec<ConnId> = self.conns.keys().copied().collect();
        for id in ids {
            self.server.send(id, &ServerMsg::Kick(reason.to_owned()));
            self.server.disconnect(id);
        }
        for c in self.conns.values() {
            if let Some(e) = c.entity {
                session.sim.entities.destroy(e);
            }
        }
        self.conns.clear();
        session.world.set_extra_centers(Vec::new());
        session.sim.entities.extra_origins.clear();
        session.world.chunk_manager().enable_edit_log(false);
        self.beacon = None;
        self.server.shutdown();
    }
}
