//! Partie en réseau local : héberger (menu pause « OPEN TO LAN »), rejoindre (écran
//! Multiplayer), quitter, et le branchement du réseau sur le tick du jeu. La logique réseau
//! elle-même est dans `crate::net` ; ici se trouve ce qui touche à l'état du jeu.

use nb_core::glam::DVec3;
use nb_ecs::{GameMode, PlayerInput, Schedule};
use nb_net::{sanitize_name, Welcome};

use super::*;
use crate::chat::ChatColor;
use crate::net::client::{ClientNotice, Phase};
use crate::net::host::{HostCtx, HostOptions};
use crate::net::{
    parse_address, Avatar, ClientNet, HostNet, LanBrowser, LanRole, LanView, NetMode,
};

/// Message affiché à un client qui tape une commande.
const HOST_ONLY: &str = "Only the host can use commands in a LAN game.";

/// Pieds, lacet et tangage du joueur local (annoncés aux autres joueurs).
fn local_pose(session: &WorldSession) -> (DVec3, f32, f32) {
    let cam = session.camera();
    let feet = session
        .player()
        .and_then(|p| session.sim.entities.copy::<nb_ecs::Transform>(p))
        .map_or(DVec3::ZERO, |t| t.pos);
    (feet, cam.yaw_deg, cam.pitch_deg)
}

impl Game {
    // ── Accès ───────────────────────────────────────────────────────────────────────────

    /// Rôle de ce jeu dans la partie réseau (aucun = solo).
    pub fn lan_role(&self) -> LanRole {
        self.net.role()
    }

    /// Ce que l'interface sait de la partie réseau.
    pub fn lan_view(&self) -> &LanView {
        &self.lan_view
    }

    /// Autres joueurs à dessiner.
    pub(super) fn avatars(&self) -> Vec<Avatar> {
        match &self.net {
            NetMode::Off => Vec::new(),
            NetMode::Host(h) => h.avatars(),
            NetMode::Client(c) => c.avatars(),
        }
    }

    /// Le client attend d'avoir assez de terrain autour de lui pour ne pas tomber dans le vide.
    pub(super) fn lan_waiting_terrain(&self) -> bool {
        matches!(&self.net, NetMode::Client(c) if c.phase() == Phase::Joined && !c.terrain_ready())
    }

    /// Une partie réseau est en cours : le monde continue de tourner pendant les menus.
    pub(super) fn lan_active(&self) -> bool {
        matches!(&self.net, NetMode::Host(_))
            || matches!(&self.net, NetMode::Client(c) if c.phase() == Phase::Joined)
    }

    pub(super) fn is_lan_client(&self) -> bool {
        matches!(&self.net, NetMode::Client(c) if c.phase() == Phase::Joined)
    }

    // ── Héberger ────────────────────────────────────────────────────────────────────────

    /// Menu pause « OPEN TO LAN » : le monde courant devient accessible aux autres ordinateurs.
    pub fn open_to_lan(&mut self) {
        if !matches!(self.net, NetMode::Off) {
            return;
        }
        let Some(world_name) = self.session.name.clone() else {
            self.chat.push(
                "Open a saved world to share it on the network",
                ChatColor::Yellow,
            );
            return;
        };
        let host_name = sanitize_name(&self.settings.player_name);
        let mut opts = HostOptions {
            port: self.opts.lan_port,
            discovery_port: self.opts.lan_discovery_port,
            host_name,
            world_name,
        };
        let started = match HostNet::start(opts.clone(), &self.regs) {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && opts.port != 0 => {
                // Port par défaut pris (deuxième jeu sur le même ordinateur) : un port libre.
                opts.port = 0;
                HostNet::start(opts, &self.regs)
            }
            r => r,
        };
        match started {
            Ok(host) => {
                self.session.world.chunk_manager().enable_edit_log(true);
                self.chat.set_net_mode(true);
                self.chat
                    .push("Your game is open to LAN", ChatColor::Yellow);
                for a in host.addresses() {
                    self.chat
                        .push(format!("Friends can join with {a}"), ChatColor::Yellow);
                }
                if let Some(p) = &host.discovery_problem {
                    self.chat.push(p.clone(), ChatColor::Yellow);
                }
                self.chat.push(
                    "Press T to chat, Tab for the player list",
                    ChatColor::Yellow,
                );
                self.net = NetMode::Host(Box::new(host));
            }
            Err(e) => {
                self.chat.push(
                    format!("Could not open the game to LAN: {e}"),
                    ChatColor::Yellow,
                );
                self.fail("ouverture au réseau local", e);
            }
        }
    }

    /// Menu pause « CLOSE LAN » : retour au solo, les invités sont renvoyés.
    pub fn close_lan(&mut self) {
        if matches!(self.net, NetMode::Host(_)) {
            self.net_stop();
            self.chat.push("LAN game closed", ChatColor::Yellow);
        }
    }

    // ── Rejoindre ───────────────────────────────────────────────────────────────────────

    /// Écran Multiplayer : connexion à `address` (`hôte[:port]`) sous le nom `name`.
    pub fn join_lan(&mut self, address: &str, name: &str) {
        if !matches!(self.net, NetMode::Off) {
            return;
        }
        let target = match parse_address(address) {
            Ok(t) => t,
            Err(e) => {
                self.lan_message = Some((e, true));
                return;
            }
        };
        let name = sanitize_name(name);
        self.settings.player_name = name.clone();
        self.settings.last_server = target.clone();
        self.save_settings();
        self.lan_message = None;
        self.net = NetMode::Client(Box::new(ClientNet::connect(
            &target,
            &name,
            self.settings.render_distance,
        )));
    }

    /// Écran Multiplayer : abandon de la connexion en cours.
    pub fn cancel_join(&mut self) {
        let pending = matches!(&self.net, NetMode::Client(c) if c.phase() != Phase::Joined);
        if pending {
            if let NetMode::Client(c) = &mut self.net {
                c.leave();
            }
            self.net = NetMode::Off;
            self.lan_message = None;
        }
    }

    /// L'hôte a accepté : le monde du client est bâti à partir de ce qu'il annonce.
    fn begin_client_session(&mut self, w: Welcome) {
        let storage = match &self.net {
            NetMode::Client(c) => c.storage(),
            _ => return,
        };
        let cfg = serde_json::from_str::<WorldConfig>(&w.config_json).unwrap_or_default();
        // Univers et palette de blocs de l'hôte : les chunks reçus sont dans ses identifiants.
        let Some(kind) = nb_data::WorldKind::parse(&w.world_kind) else {
            let why = format!(
                "The host's world uses the \"{}\" universe, which your version of Norrblock does not have: update Norrblock",
                w.world_kind
            );
            return self.leave_client(Some((why, true)));
        };
        let palette = nb_data::BlockPalette::new(w.block_palette.clone());
        let unknown = match self.use_registries(kind, &palette) {
            Ok(report) => report.unknown,
            Err(e) => {
                let why = format!("Could not use the host's blocks: {e}");
                return self.leave_client(Some((why, true)));
            }
        };
        self.release_session_core();
        let fov = self.session.camera().fov;
        let rngs = ecs_rngs(&self.opts, &mut self.rng);
        let mode = if w.creative {
            GameMode::Creative
        } else {
            GameMode::Survival
        };
        let mut s = WorldSession::new(
            w.seed,
            storage,
            cfg.clone(),
            &self.regs,
            menu_camera(fov),
            mode,
            rngs,
        );
        s.sim.schedule = Schedule::client();
        s.allow_cheats = w.allow_cheats;
        let cm = s.world.chunk_manager();
        cm.set_remote(true);
        cm.enable_edit_log(true);
        cm.set_render_distance(self.settings.render_distance);
        s.place_player(w.spawn);
        self.session = s;
        self.sky.set_day_length_seconds(w.day_length);
        self.sky.set_time(w.time_of_day);
        self.cursor.reset();
        self.chat.set_net_mode(true);
        self.chat.push(
            format!("Joined {}'s world \"{}\"", w.host_name, w.world_name),
            ChatColor::Yellow,
        );
        if !unknown.is_empty() {
            self.chat.push(
                format!(
                    "Blocks unknown to this version (shown as missing): {}",
                    unknown.join(", ")
                ),
                ChatColor::Yellow,
            );
        }
        self.chat.push(
            "Press T to chat, Tab for the player list",
            ChatColor::Yellow,
        );
        self.lan_message = None;
        self.switch_state(GameState::Playing);
    }

    /// Fin de la partie côté client. Avec un message : retour à l'écran Multiplayer qui
    /// l'affiche ; sans : retour au menu principal.
    pub(super) fn leave_client(&mut self, notice: Option<(String, bool)>) {
        let joined = self.is_lan_client();
        self.net_stop();
        if joined {
            self.release_session_core();
            self.install_menu_world();
        }
        self.cursor.reset();
        match notice {
            Some(n) => {
                self.lan_message = Some(n);
                self.switch_state(GameState::Multiplayer);
            }
            None => self.switch_state(GameState::MainMenu),
        }
    }

    // ── Arrêt ───────────────────────────────────────────────────────────────────────────

    /// Ferme la partie réseau (hôte : invités prévenus ; client : déconnexion). Sans effet en solo.
    pub(super) fn net_stop(&mut self) {
        match std::mem::take(&mut self.net) {
            NetMode::Off => return,
            NetMode::Host(mut h) => h.close(&mut self.session, "The host closed the game"),
            NetMode::Client(mut c) => c.leave(),
        }
        self.chat.set_net_mode(false);
        self.session.world.chunk_manager().enable_edit_log(false);
        self.lan_view = LanView::default();
    }

    // ── Tick ────────────────────────────────────────────────────────────────────────────

    /// Avant la simulation : messages reçus.
    pub(super) fn net_pre_tick(&mut self, dt: f64) {
        let dt = dt as f32;
        let mut lines: Vec<(String, bool)> = Vec::new();
        let mut loot: Vec<(nb_data::ItemId, i32)> = Vec::new();
        let mut joined = None;
        let mut closed = None;
        match &mut self.net {
            NetMode::Off => return,
            NetMode::Host(h) => {
                let pose = local_pose(&self.session);
                let mut cx = HostCtx {
                    session: &mut self.session,
                    regs: &self.regs,
                    sky: &self.sky,
                    host_pose: pose,
                    chat: &mut lines,
                };
                h.pre_tick(&mut cx, dt);
            }
            NetMode::Client(c) => {
                let mut notices = c.poll();
                if let Some(i) = notices
                    .iter()
                    .position(|n| matches!(n, ClientNotice::Joined(_)))
                {
                    if let ClientNotice::Joined(w) = notices.remove(i) {
                        joined = Some(*w);
                    }
                }
                if joined.is_none() && c.phase() == Phase::Joined {
                    notices.extend(c.apply(&mut self.session, &self.regs, &mut self.sky, dt));
                }
                for n in notices {
                    match n {
                        ClientNotice::Chat { from, text } => lines.push(if from.is_empty() {
                            (text, true)
                        } else {
                            (format!("<{from}> {text}"), false)
                        }),
                        ClientNotice::Closed { reason, error } => closed = Some((reason, error)),
                        ClientNotice::Gained { item, count } => loot.push((item, count)),
                        ClientNotice::Joined(_) => {}
                    }
                }
            }
        }
        self.push_net_lines(lines);
        for (item, count) in loot {
            self.note_pickup(item, count);
        }
        if let Some(w) = joined {
            self.begin_client_session(w);
        } else if let Some(n) = closed {
            let was_playing = self.is_lan_client();
            if was_playing {
                self.leave_client(Some(n));
            } else {
                // Refus ou échec de connexion : on reste sur l'écran Multiplayer.
                self.net_stop();
                self.lan_message = Some(n);
            }
        }
    }

    /// Après la simulation : état annoncé aux autres.
    pub(super) fn net_post_tick(&mut self, dt: f64, ev_start: usize) {
        let dt = dt as f32;
        let mut lines: Vec<(String, bool)> = Vec::new();
        match &mut self.net {
            NetMode::Off => return,
            NetMode::Host(h) => {
                let pose = local_pose(&self.session);
                let mut cx = HostCtx {
                    session: &mut self.session,
                    regs: &self.regs,
                    sky: &self.sky,
                    host_pose: pose,
                    chat: &mut lines,
                };
                h.post_tick(&mut cx, dt);
            }
            NetMode::Client(c) => {
                let start = ev_start.min(self.events.len());
                c.post_tick(&mut self.session, &self.events[start..], dt);
            }
        }
        self.push_net_lines(lines);
    }

    fn push_net_lines(&mut self, lines: Vec<(String, bool)>) {
        for (text, system) in lines {
            let color = if system {
                ChatColor::Yellow
            } else {
                ChatColor::White
            };
            self.chat.push(text, color);
        }
    }

    /// Tick de la simulation pendant les menus d'une partie réseau (le monde ne s'arrête pas).
    pub(super) fn lan_idle_tick(&mut self, dt: f64) {
        if !self.lan_active() || self.lan_waiting_terrain() {
            return;
        }
        let pi = PlayerInput {
            mouse_locked: false,
            selected_slot: self.input.selected_slot(),
            ..PlayerInput::default()
        };
        self.session
            .tick(&pi, &self.regs, &mut self.events, dt as f32);
    }

    // ── Chat ────────────────────────────────────────────────────────────────────────────

    /// Texte validé dans le chat : commande (« / ») ou message pour les autres joueurs.
    /// Renvoie la réponse affichée à l'auteur.
    pub(super) fn chat_submit(
        session: &mut WorldSession,
        sky: &mut SkyClock,
        role: LanRole,
        outgoing: &mut Vec<String>,
        text: &str,
    ) -> Option<String> {
        if role == LanRole::Off || text.starts_with('/') {
            if role == LanRole::Client && !text.trim_start().starts_with("/help") {
                return Some(HOST_ONLY.to_owned());
            }
            return session.execute_command(text, sky);
        }
        if !text.trim().is_empty() {
            outgoing.push(text.to_owned());
        }
        None
    }

    /// Envoie les messages saisis pendant ce tick.
    pub(super) fn net_send_chat(&mut self, outgoing: Vec<String>) {
        for text in outgoing {
            match &mut self.net {
                NetMode::Host(h) => {
                    h.host_chat(&text);
                    let me = sanitize_name(&self.settings.player_name);
                    let shown: String =
                        text.trim().chars().take(crate::net::MAX_CHAT_LEN).collect();
                    self.chat.push(format!("<{me}> {shown}"), ChatColor::White);
                }
                NetMode::Client(c) => c.send_chat(&text),
                NetMode::Off => {}
            }
        }
    }

    // ── Interface ───────────────────────────────────────────────────────────────────────

    /// Met à jour [`LanView`] pour l'interface.
    pub(super) fn refresh_lan_view(&mut self) {
        if self.state == GameState::Multiplayer {
            self.lan_browser.poll();
        }
        let mut v = LanView {
            role: self.net.role(),
            message: self.lan_message.clone(),
            ..LanView::default()
        };
        match &self.net {
            NetMode::Off => {}
            NetMode::Host(h) => {
                v.addresses = h.addresses();
                v.players = h.players();
                if let Some(me) = v.players.first_mut() {
                    me.is_self = true;
                }
            }
            NetMode::Client(c) => {
                if c.phase() == Phase::Joined {
                    v.players = c.players();
                    v.loading_terrain = !c.terrain_ready();
                    v.terrain_progress = Some(c.terrain_progress());
                } else {
                    v.connecting = Some(c.target().to_owned());
                }
            }
        }
        if self.state == GameState::Multiplayer {
            v.servers = self.lan_browser.servers();
            if v.message.is_none() {
                v.message = self.lan_browser.problem.clone().map(|p| (p, true));
            }
        }
        self.lan_view = v;
    }

    /// Chunks à mailler : ceux de la distance de rendu du joueur local (l'hôte garde chargés
    /// d'autres chunks pour ses invités ; il n'a pas à les dessiner).
    pub(super) fn visible_chunks(&self, all: Vec<nb_core::ChunkPos>) -> Vec<nb_core::ChunkPos> {
        if !matches!(self.net, NetMode::Host(_)) {
            return all;
        }
        let cm = self.session.world.chunk_manager();
        let rd = cm.render_distance();
        let c = crate::net::chunk_of(
            self.session.camera().position.x.floor() as i32,
            self.session.camera().position.z.floor() as i32,
        );
        all.into_iter()
            .filter(|p| crate::net::chunk_dist(*p, c) <= rd)
            .collect()
    }
}

impl LanBrowser {
    /// Navigateur de parties à la demande de [`GameOptions`].
    pub(super) fn for_options(opts: &GameOptions) -> Self {
        match opts.lan_discovery_port {
            Some(p) => Self::with_discovery_port(p),
            None => Self::default(),
        }
    }
}
