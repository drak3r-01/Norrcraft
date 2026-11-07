//! Partie en réseau local côté jeu : l'hôte (serveur intégré), le client, la recherche de
//! parties. Le transport est dans `nb-net` ; ici se trouve la logique de jeu répliquée.
//!
//! Principe (voir `docs/LAN.md`) :
//!
//! * **un joueur héberge** : son jeu fait tourner le monde (chunks, mobs, apparitions) comme en
//!   solo et accepte jusqu'à [`nb_net::MAX_PLAYERS`] joueurs ; aucun serveur dédié ;
//! * **l'hôte fait foi** pour le monde et les mobs ; chaque joueur fait foi pour sa propre
//!   position, son inventaire et sa santé (réseau de confiance : amis sur le même LAN) ;
//! * un client ne génère rien : l'hôte lui envoie les chunks autour de lui, puis chaque bloc
//!   modifié ; les mobs sont répliqués à 20 Hz et lissés à l'affichage.

use std::collections::HashSet;

use nb_core::glam::DVec3;
use nb_core::ChunkPos;

pub mod client;
pub mod host;
pub mod lan;

pub use client::ClientNet;
pub use host::HostNet;
pub use lan::{parse_address, LanBrowser};

/// Cadence d'envoi des instantanés et des positions.
pub const NET_HZ: f32 = 20.0;
/// Intervalle entre deux instantanés.
pub const SNAPSHOT_INTERVAL: f32 = 1.0 / NET_HZ;
/// Les mobs au-delà de cette distance horizontale ne sont pas répliqués.
pub const MOB_SYNC_RADIUS: f64 = 96.0;
/// Portée maximale (blocs) d'une modification de bloc ou d'un coup acceptés par l'hôte.
pub const MAX_REACH: f64 = 12.0;
/// Délai sans nouvelles au-delà duquel une connexion est considérée perdue.
pub const TIMEOUT_SECS: u64 = 20;
/// Délai laissé à un nouveau venu pour se présenter.
pub const HANDSHAKE_SECS: u64 = 6;
/// Longueur maximale d'un message de chat.
pub const MAX_CHAT_LEN: usize = 200;

/// Chunk contenant la colonne `(x, z)`.
pub fn chunk_of(x: i32, z: i32) -> ChunkPos {
    ChunkPos::new(
        nb_core::math::floor_div(x, 16),
        nb_core::math::floor_div(z, 16),
    )
}

/// Distance de Chebyshev entre deux chunks.
pub fn chunk_dist(a: ChunkPos, b: ChunkPos) -> i32 {
    (a.x - b.x).abs().max((a.z - b.z).abs())
}

/// Un autre joueur, tel qu'il est dessiné.
#[derive(Clone, Debug, PartialEq)]
pub struct Avatar {
    pub id: u16,
    pub name: String,
    /// Pieds.
    pub pos: DVec3,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
}

/// Position et angles lissés vers la dernière valeur reçue.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Smooth {
    pub pos: DVec3,
    pub target: DVec3,
    pub yaw: f32,
    pub target_yaw: f32,
    pub pitch: f32,
    pub target_pitch: f32,
}

/// Angle (degrés) vers lequel tourner par le plus court chemin.
pub(crate) fn shortest_delta_deg(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

impl Smooth {
    pub fn new(pos: DVec3, yaw: f32, pitch: f32) -> Self {
        Self {
            pos,
            target: pos,
            yaw,
            target_yaw: yaw,
            pitch,
            target_pitch: pitch,
        }
    }

    pub fn set_target(&mut self, pos: DVec3, yaw: f32, pitch: f32) {
        // Téléportation (mort, /kill) : pas de glissade à travers le monde.
        if (pos - self.target).length_squared() > 40.0 * 40.0 {
            self.pos = pos;
        }
        self.target = pos;
        self.target_yaw = yaw;
        self.target_pitch = pitch;
    }

    pub fn step(&mut self, dt: f32) {
        let k = 1.0 - (-14.0 * f64::from(dt)).exp();
        self.pos += (self.target - self.pos) * k;
        let kf = k as f32;
        self.yaw += shortest_delta_deg(self.yaw, self.target_yaw) * kf;
        self.pitch += (self.target_pitch - self.pitch) * kf;
    }
}

/// Rôle de ce jeu dans une partie LAN.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LanRole {
    #[default]
    Off,
    Host,
    Client,
}

/// Joueur de la liste (touche Tab).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEntry {
    pub name: String,
    pub is_self: bool,
    pub is_host: bool,
    /// Latence aller-retour (absente pour soi-même).
    pub ping_ms: Option<u32>,
}

/// Partie trouvée par la recherche.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEntry {
    pub world: String,
    pub host: String,
    /// `adresse:port` à joindre.
    pub address: String,
    pub players: u8,
    pub max_players: u8,
}

/// Ce que l'interface sait de la partie en réseau.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LanView {
    pub role: LanRole,
    /// Adresses où joindre l'hôte (`ip:port`), si on héberge.
    pub addresses: Vec<String>,
    pub players: Vec<PlayerEntry>,
    pub servers: Vec<ServerEntry>,
    /// Connexion en cours (`Some("192.168.1.5:25570")`).
    pub connecting: Option<String>,
    /// Dernier message d'état (texte, est-ce une erreur).
    pub message: Option<(String, bool)>,
    /// Le client attend les premiers chunks de l'hôte.
    pub loading_terrain: bool,
    /// Nombre de chunks reçus / attendus à l'arrivée (progression affichée).
    pub terrain_progress: Option<(usize, usize)>,
}

/// Mode réseau du jeu.
#[derive(Default)]
pub enum NetMode {
    #[default]
    Off,
    Host(Box<HostNet>),
    Client(Box<ClientNet>),
}

impl std::fmt::Debug for NetMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetMode::Off => write!(f, "Off"),
            NetMode::Host(_) => write!(f, "Host"),
            NetMode::Client(_) => write!(f, "Client"),
        }
    }
}

impl NetMode {
    pub fn role(&self) -> LanRole {
        match self {
            NetMode::Off => LanRole::Off,
            NetMode::Host(_) => LanRole::Host,
            NetMode::Client(_) => LanRole::Client,
        }
    }
}

/// Ordre d'envoi des chunks : décalages triés du plus proche au plus éloigné.
pub(crate) fn ring_offsets(radius: i32) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| (x, z)))
        .collect();
    v.sort_by_key(|&(x, z)| (x * x + z * z, x, z));
    v
}

/// Ensemble de chunks à garder en mémoire côté hôte pour un client.
pub(crate) type Known = HashSet<ChunkPos>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_of_floors_negative_coordinates() {
        assert_eq!(chunk_of(0, 15), ChunkPos::new(0, 0));
        assert_eq!(chunk_of(-1, 16), ChunkPos::new(-1, 1));
        assert_eq!(chunk_of(-16, -17), ChunkPos::new(-1, -2));
        assert_eq!(chunk_dist(ChunkPos::new(0, 0), ChunkPos::new(-3, 2)), 3);
    }

    #[test]
    fn angles_take_the_short_way() {
        assert!((shortest_delta_deg(350.0, 10.0) - 20.0).abs() < 1e-4);
        assert!((shortest_delta_deg(10.0, 350.0) + 20.0).abs() < 1e-4);
        assert!(shortest_delta_deg(0.0, 180.0).abs() > 179.0);
        let mut s = Smooth::new(DVec3::ZERO, 350.0, 0.0);
        s.set_target(DVec3::new(10.0, 0.0, 0.0), 10.0, 0.0);
        for _ in 0..120 {
            s.step(1.0 / 60.0);
        }
        assert!((s.pos.x - 10.0).abs() < 0.01);
        assert!(shortest_delta_deg(s.yaw, 10.0).abs() < 0.1, "yaw {}", s.yaw);
    }

    #[test]
    fn teleports_do_not_glide() {
        let mut s = Smooth::new(DVec3::ZERO, 0.0, 0.0);
        s.set_target(DVec3::new(500.0, 70.0, 0.0), 0.0, 0.0);
        assert_eq!(s.pos.x, 500.0);
    }

    #[test]
    fn rings_start_at_the_center_and_cover_the_square() {
        let r = ring_offsets(3);
        assert_eq!(r[0], (0, 0));
        assert_eq!(r.len(), 49);
        assert!(r.windows(2).all(|w| {
            let d = |(x, z): (i32, i32)| x * x + z * z;
            d(w[0]) <= d(w[1])
        }));
    }
}
