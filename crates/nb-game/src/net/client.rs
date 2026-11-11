//! Client d'une partie LAN : connexion à l'hôte, monde répliqué (chunks, blocs, mobs, autres
//! joueurs). Le client ne génère rien et ne simule aucun mob : il affiche ce que l'hôte annonce
//! et lui transmet ce que fait son joueur.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_core::glam::DVec3;
use nb_core::ChunkPos;
use nb_ecs::{Collider, EcsEvent, EntityId, Health, MobType, NetMob, Player, Transform, Velocity};
use nb_gfx::SkyClock;
use nb_net::{
    ClientEvent, ClientMsg, MobPose, NetClient, PlayerPose, ServerMsg, Welcome, PROTOCOL_VERSION,
};
use nb_world::storage::decode_chunk;
use nb_world::InMemoryChunkStorage;

use super::{
    chunk_dist, chunk_of, shortest_delta_deg, Avatar, PlayerEntry, Smooth, SNAPSHOT_INTERVAL,
    TIMEOUT_SECS,
};
use crate::session::{Registries, WorldSession};

/// Étape de la connexion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Connexion TCP en cours.
    Connecting,
    /// Connecté, « Hello » envoyé, on attend le « Welcome ».
    Greeting,
    /// Dans le monde de l'hôte.
    Joined,
}

/// Ce que le jeu doit faire à la suite d'un événement réseau.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientNotice {
    /// L'hôte accepte : construire le monde client.
    Joined(Box<Welcome>),
    /// Ligne de chat (`from` vide = message système).
    Chat { from: String, text: String },
    /// Fin de la partie (refus, hôte parti, connexion perdue). `error` = à signaler en rouge.
    Closed { reason: String, error: bool },
    /// Butin d'un mob tué (déjà ajouté à l'inventaire).
    Gained { item: nb_data::ItemId, count: i32 },
}

struct Other {
    name: String,
    shown: Option<Smooth>,
}

/// Connexion et réplication côté client.
pub struct ClientNet {
    client: NetClient,
    phase: Phase,
    target: String,
    name: String,
    render_distance: i32,
    last_rx: Instant,
    inbox: VecDeque<ServerMsg>,
    player_id: u16,
    host_name: String,
    world_name: String,
    mob_types: Vec<String>,
    others: BTreeMap<u16, Other>,
    storage: Arc<InMemoryChunkStorage>,
    mobs: HashMap<u32, EntityId>,
    pending_edits: HashMap<ChunkPos, Vec<(i32, i32, i32, u8)>>,
    state_timer: f32,
    ping_timer: f32,
    ping_sent: Option<(u32, Instant)>,
    ping_seq: u32,
    ping_ms: Option<u32>,
    prune_timer: f32,
    chunks_received: usize,
    terrain_ready: bool,
}

impl std::fmt::Debug for ClientNet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientNet")
            .field("target", &self.target)
            .field("phase", &self.phase)
            .finish()
    }
}

impl ClientNet {
    /// Lance la connexion à `target` (`hôte:port`) sans bloquer.
    pub fn connect(target: &str, name: &str, render_distance: i32) -> Self {
        Self {
            client: NetClient::connect(target.to_owned()),
            phase: Phase::Connecting,
            target: target.to_owned(),
            name: name.to_owned(),
            render_distance: render_distance.clamp(2, 16),
            last_rx: Instant::now(),
            inbox: VecDeque::new(),
            player_id: 0,
            host_name: String::new(),
            world_name: String::new(),
            mob_types: Vec::new(),
            others: BTreeMap::new(),
            storage: Arc::new(InMemoryChunkStorage::new()),
            mobs: HashMap::new(),
            pending_edits: HashMap::new(),
            state_timer: 0.0,
            ping_timer: 0.0,
            ping_sent: None,
            ping_seq: 0,
            ping_ms: None,
            prune_timer: 0.0,
            chunks_received: 0,
            terrain_ready: false,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    pub fn world_name(&self) -> &str {
        &self.world_name
    }

    pub fn player_id(&self) -> u16 {
        self.player_id
    }

    pub fn ping_ms(&self) -> Option<u32> {
        self.ping_ms
    }

    pub fn terrain_ready(&self) -> bool {
        self.terrain_ready
    }

    /// Stockage mémoire où arrivent les chunks (donné au `ChunkManager` du monde client).
    pub fn storage(&self) -> Arc<InMemoryChunkStorage> {
        self.storage.clone()
    }

    /// Chunks reçus / chunks attendus à la distance de rendu.
    pub fn terrain_progress(&self) -> (usize, usize) {
        let side = (2 * self.render_distance + 1) as usize;
        (self.chunks_received.min(side * side), side * side)
    }

    /// Joueurs de la partie (soi-même compris), l'hôte en tête.
    pub fn players(&self) -> Vec<PlayerEntry> {
        let mut v = vec![PlayerEntry {
            name: self.host_name.clone(),
            is_self: false,
            is_host: true,
            ping_ms: None,
        }];
        v.extend(
            self.others
                .iter()
                .filter(|(&id, _)| id != 0)
                .map(|(_, o)| PlayerEntry {
                    name: o.name.clone(),
                    is_self: false,
                    is_host: false,
                    ping_ms: None,
                }),
        );
        v.push(PlayerEntry {
            name: self.name.clone(),
            is_self: true,
            is_host: false,
            ping_ms: self.ping_ms,
        });
        v
    }

    pub fn avatars(&self) -> Vec<Avatar> {
        self.others
            .iter()
            .filter_map(|(&id, o)| {
                let s = o.shown?;
                Some(Avatar {
                    id,
                    name: o.name.clone(),
                    pos: s.pos,
                    yaw_deg: s.yaw,
                    pitch_deg: s.pitch,
                })
            })
            .collect()
    }

    /// Relève le réseau. Les messages du monde attendent dans la file jusqu'à [`Self::apply`].
    pub fn poll(&mut self) -> Vec<ClientNotice> {
        let mut out = Vec::new();
        for ev in self.client.poll() {
            self.last_rx = Instant::now();
            match ev {
                ClientEvent::Connected => {
                    self.phase = Phase::Greeting;
                    self.client.send(&ClientMsg::Hello {
                        version: PROTOCOL_VERSION,
                        name: self.name.clone(),
                        render_distance: self.render_distance as u8,
                        content: crate::session::content_fingerprints(),
                    });
                }
                ClientEvent::Disconnected(reason) => out.push(ClientNotice::Closed {
                    reason: friendly_disconnect(&reason),
                    error: true,
                }),
                ClientEvent::Message(ServerMsg::Welcome(w)) => {
                    self.phase = Phase::Joined;
                    self.player_id = w.player_id;
                    self.name = w.name.clone();
                    self.host_name = w.host_name.clone();
                    self.world_name = w.world_name.clone();
                    self.mob_types = w.mob_types.clone();
                    out.push(ClientNotice::Joined(w));
                }
                ClientEvent::Message(ServerMsg::Refuse(reason)) => out.push(ClientNotice::Closed {
                    reason,
                    error: true,
                }),
                ClientEvent::Message(ServerMsg::Kick(reason)) => out.push(ClientNotice::Closed {
                    reason: if reason.is_empty() {
                        "The host closed the game".into()
                    } else {
                        reason
                    },
                    error: false,
                }),
                ClientEvent::Message(m) => self.inbox.push_back(m),
            }
        }
        let limit = Duration::from_secs(TIMEOUT_SECS);
        if self.phase != Phase::Joined && self.last_rx.elapsed() > limit {
            out.push(ClientNotice::Closed {
                reason: "The host did not answer".into(),
                error: true,
            });
        } else if self.phase == Phase::Joined && self.last_rx.elapsed() > limit {
            out.push(ClientNotice::Closed {
                reason: "Connection to the host timed out".into(),
                error: true,
            });
        }
        out
    }

    /// Applique les messages du monde reçus (monde client déjà construit).
    pub fn apply(
        &mut self,
        session: &mut WorldSession,
        regs: &Registries,
        sky: &mut SkyClock,
        dt: f32,
    ) -> Vec<ClientNotice> {
        let mut out = Vec::new();
        while let Some(m) = self.inbox.pop_front() {
            match m {
                ServerMsg::Chunk { cx, cz, data } => self.on_chunk(session, cx, cz, &data),
                ServerMsg::BlockChange { pos, block } => self.on_block(session, pos, block),
                ServerMsg::Snapshot { players, mobs } => {
                    self.on_players(&players);
                    self.on_mobs(session, regs, &mobs);
                }
                ServerMsg::PlayerJoined { id, name } => {
                    self.others.insert(id, Other { name, shown: None });
                }
                ServerMsg::PlayerLeft { id } => {
                    self.others.remove(&id);
                }
                ServerMsg::SetHealth(h) => {
                    if let Some(p) = session.player() {
                        if let Some(hp) = session.sim.entities.get_mut::<Health>(p) {
                            hp.health = h.clamp(0.0, hp.max_health);
                            hp.alive = hp.health > 0.0;
                        }
                    }
                }
                ServerMsg::Time {
                    time_of_day,
                    day_length,
                } => {
                    sky.set_day_length_seconds(day_length);
                    sky.set_time(time_of_day);
                }
                ServerMsg::Chat { from, text } => out.push(ClientNotice::Chat { from, text }),
                ServerMsg::MobKilled(mob) => {
                    // Butin tiré chez l'invité (son inventaire est local).
                    if let Some(p) = session.player() {
                        let loot = regs.survival.mob_loot(&mob).to_vec();
                        let mut gained = Vec::new();
                        nb_ecs::survival::loot_direct(
                            &mut session.sim.entities,
                            p,
                            &loot,
                            &mut session.rngs.ambient,
                            &mut gained,
                        );
                        for (item, count) in gained {
                            out.push(ClientNotice::Gained { item, count });
                        }
                    }
                }
                ServerMsg::Pong(n) => {
                    if let Some((sent_n, at)) = self.ping_sent {
                        if sent_n == n {
                            self.ping_ms = Some(at.elapsed().as_millis().min(9999) as u32);
                            self.ping_sent = None;
                        }
                    }
                }
                // Déjà traités par `poll`.
                ServerMsg::Welcome(_) | ServerMsg::Refuse(_) | ServerMsg::Kick(_) => {}
            }
        }
        self.apply_pending_edits(session);
        self.smooth(session, dt);
        self.update_terrain_ready(session);
        out
    }

    fn player_chunk(&self, session: &WorldSession) -> ChunkPos {
        let pos = session.camera().position;
        chunk_of(pos.x.floor() as i32, pos.z.floor() as i32)
    }

    fn on_chunk(&mut self, session: &mut WorldSession, cx: i32, cz: i32, data: &[u8]) {
        let pos = ChunkPos::new(cx, cz);
        if chunk_dist(pos, self.player_chunk(session)) > self.render_distance + 3 {
            return; // arrivé trop tard : le joueur s'est éloigné
        }
        match decode_chunk(data) {
            Ok(vox) => {
                self.chunks_received += 1;
                session.world.chunk_manager().install_chunk(pos, &vox);
                // Les modifications arrivées avant l'activation sont déjà dans les données de
                // l'hôte ; celles reçues après sont rejouées par `apply_pending_edits`.
            }
            Err(e) => eprintln!("réseau : chunk {pos:?} illisible : {e}"),
        }
    }

    fn on_block(&mut self, session: &mut WorldSession, pos: [i32; 3], block: u8) {
        let [x, y, z] = pos;
        let cm = session.world.chunk_manager();
        if !cm.set_block_quiet(x, y, z, block) {
            // Chunk reçu mais pas encore actif : on rejouera la modification à son activation.
            self.pending_edits
                .entry(chunk_of(x, z))
                .or_default()
                .push((x, y, z, block));
        }
    }

    fn apply_pending_edits(&mut self, session: &mut WorldSession) {
        if self.pending_edits.is_empty() {
            return;
        }
        let cm = session.world.chunk_manager();
        let center = self.player_chunk(session);
        let keep = self.render_distance + 3;
        self.pending_edits.retain(|&pos, edits| {
            if cm.chunk(pos).is_some() {
                for &(x, y, z, b) in edits.iter() {
                    cm.set_block_quiet(x, y, z, b);
                }
                false
            } else {
                chunk_dist(pos, center) <= keep
            }
        });
    }

    fn on_players(&mut self, poses: &[PlayerPose]) {
        for p in poses {
            let pos = DVec3::new(
                f64::from(p.pos[0]),
                f64::from(p.pos[1]),
                f64::from(p.pos[2]),
            );
            let other = self.others.entry(p.id).or_insert_with(|| Other {
                name: format!("Player {}", p.id),
                shown: None,
            });
            match &mut other.shown {
                Some(s) => s.set_target(pos, p.yaw, p.pitch),
                None => other.shown = Some(Smooth::new(pos, p.yaw, p.pitch)),
            }
        }
        // Un joueur absent de l'instantané (hors de portée ou parti) n'est plus dessiné.
        for (id, o) in &mut self.others {
            if !poses.iter().any(|p| p.id == *id) {
                o.shown = None;
            }
        }
    }

    fn on_mobs(&mut self, session: &mut WorldSession, regs: &Registries, poses: &[MobPose]) {
        let e = &mut session.sim.entities;
        let mut seen = std::collections::HashSet::with_capacity(poses.len());
        for m in poses {
            let Some(mob_id) = self.mob_types.get(usize::from(m.kind)) else {
                continue;
            };
            seen.insert(m.id);
            let pos = DVec3::new(
                f64::from(m.pos[0]),
                f64::from(m.pos[1]),
                f64::from(m.pos[2]),
            );
            let ent = match self.mobs.get(&m.id) {
                Some(&ent) if e.contains(ent) => ent,
                _ => {
                    let (w, h) = regs
                        .mobs
                        .get(mob_id)
                        .map_or((0.6, 1.0), |c| (c.width_x.max(c.width_z), c.height));
                    let (wx, wz) = regs
                        .mobs
                        .get(mob_id)
                        .map_or((w, w), |c| (c.width_x, c.width_z));
                    let ent = e.create();
                    let mut t = Transform::new(pos.x, pos.y, pos.z);
                    t.yaw = m.yaw;
                    t.pitch = m.pitch;
                    e.insert(ent, t);
                    e.insert(ent, Velocity::default());
                    e.insert(ent, Collider::new(wx, wz, h, 0.0));
                    e.insert(ent, Health::new(m.health.max(1.0)));
                    e.insert(ent, MobType::new(mob_id.clone()));
                    e.insert(
                        ent,
                        NetMob {
                            net_id: m.id,
                            target: pos,
                            target_yaw: m.yaw,
                            target_pitch: m.pitch,
                        },
                    );
                    self.mobs.insert(m.id, ent);
                    ent
                }
            };
            if let Some(n) = e.get_mut::<NetMob>(ent) {
                n.target = pos;
                n.target_yaw = m.yaw;
                n.target_pitch = m.pitch;
            }
            if let Some(h) = e.get_mut::<Health>(ent) {
                if m.health > h.max_health {
                    h.max_health = m.health;
                }
                h.health = m.health;
                h.alive = m.health > 0.0;
            }
        }
        let gone: Vec<u32> = self
            .mobs
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            if let Some(ent) = self.mobs.remove(&id) {
                e.destroy(ent);
            }
        }
    }

    /// Glisse les mobs et les autres joueurs vers leur dernière position reçue.
    fn smooth(&mut self, session: &mut WorldSession, dt: f32) {
        for o in self.others.values_mut() {
            if let Some(s) = &mut o.shown {
                s.step(dt);
            }
        }
        let k = 1.0 - (-14.0 * f64::from(dt)).exp();
        let e = &mut session.sim.entities;
        for id in e.query_ids::<(&Transform, &NetMob)>() {
            let Some(n) = e.copy::<NetMob>(id) else {
                continue;
            };
            if let Some(t) = e.get_mut::<Transform>(id) {
                if (n.target - t.pos).length_squared() > 20.0 * 20.0 {
                    t.pos = n.target;
                } else {
                    t.pos += (n.target - t.pos) * k;
                }
                let d = shortest_delta_deg(t.yaw.to_degrees(), n.target_yaw.to_degrees());
                t.yaw += d.to_radians() * k as f32;
                t.pitch += (n.target_pitch - t.pitch) * k as f32;
            }
        }
    }

    fn update_terrain_ready(&mut self, session: &WorldSession) {
        if self.terrain_ready {
            return;
        }
        let cm = session.world.chunk_manager();
        let c = self.player_chunk(session);
        if cm.has_neighbors(c) {
            self.terrain_ready = true;
        }
    }

    /// Après la simulation : position, modifications de blocs, coups, latence, ménage.
    pub fn post_tick(&mut self, session: &mut WorldSession, events: &[EcsEvent], dt: f32) {
        if self.phase != Phase::Joined {
            return;
        }
        // Blocs posés / cassés par le joueur local (les écritures reçues de l'hôte ne sont
        // pas journalisées).
        for e in session.world.chunk_manager().take_edits() {
            self.client.send(&ClientMsg::BlockEdit {
                pos: [e.x, e.y, e.z],
                block: e.id,
            });
        }
        for ev in events {
            if let EcsEvent::MobHit { entity } = ev {
                if let Some(n) = session.sim.entities.copy::<NetMob>(*entity) {
                    // Dégâts de l'arme tenue (1 à mains nues / hors survie).
                    let damage = events
                        .iter()
                        .find_map(|e| match e {
                            EcsEvent::AttackDealt { entity: x, damage } if x == entity => {
                                Some(*damage)
                            }
                            _ => None,
                        })
                        .unwrap_or(1.0);
                    self.client.send(&ClientMsg::Attack {
                        entity: n.net_id,
                        damage,
                    });
                }
            }
        }
        self.state_timer += dt;
        if self.state_timer >= SNAPSHOT_INTERVAL && self.terrain_ready {
            self.state_timer = 0.0;
            if let Some(p) = session.player() {
                let e = &session.sim.entities;
                if let (Some(t), Some(pl), Some(h)) = (
                    e.copy::<Transform>(p),
                    e.copy::<Player>(p),
                    e.copy::<Health>(p),
                ) {
                    let reduction = e.copy::<nb_ecs::Vitals>(p).map_or(0.0, |v| v.reduction);
                    self.client.send(&ClientMsg::State {
                        pos: [t.pos.x, t.pos.y, t.pos.z],
                        yaw: pl.camera.yaw_deg,
                        pitch: pl.camera.pitch_deg,
                        health: h.health,
                        creative: pl.game_mode == nb_ecs::GameMode::Creative,
                        reduction,
                    });
                }
            }
        }
        self.ping_timer += dt;
        if self.ping_timer >= 2.0 {
            self.ping_timer = 0.0;
            self.ping_seq = self.ping_seq.wrapping_add(1);
            self.ping_sent = Some((self.ping_seq, Instant::now()));
            self.client.send(&ClientMsg::Ping(self.ping_seq));
        }
        self.prune_timer += dt;
        if self.prune_timer >= 1.0 {
            self.prune_timer = 0.0;
            // Chunks reçus mais plus utiles : oubliés (l'hôte aussi les oublie, voir `sync_chunks`).
            let c = self.player_chunk(session);
            for p in self.storage.positions() {
                if chunk_dist(p, c) > self.render_distance + 3 {
                    self.storage.remove(p);
                }
            }
        }
    }

    pub fn send_chat(&self, text: &str) {
        self.client.send(&ClientMsg::Chat(text.to_owned()));
    }

    /// Quitte la partie proprement.
    pub fn leave(&mut self) {
        self.client.close();
    }
}

/// Message de déconnexion lisible par le joueur.
fn friendly_disconnect(raw: &str) -> String {
    if raw.contains("refused") || raw.contains("refusée") {
        "Connection refused: is the game still open to LAN at that address?".into()
    } else if raw.contains("timed out") || raw.contains("délai") {
        "No answer from that address (check the address and that both computers are on the same network)".into()
    } else if raw.contains("connexion fermée") {
        "The host closed the game".into()
    } else {
        raw.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disconnect_reasons_are_readable() {
        assert!(
            friendly_disconnect("connexion impossible : Connection refused (os error 111)")
                .starts_with("Connection refused")
        );
        assert!(
            friendly_disconnect("connexion impossible : connection timed out")
                .contains("No answer")
        );
        assert_eq!(
            friendly_disconnect("connexion fermée par l'hôte"),
            "The host closed the game"
        );
        assert_eq!(friendly_disconnect("autre"), "autre");
    }
}
