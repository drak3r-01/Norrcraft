//! nb-net — pile réseau LAN de Norrblock : protocole binaire, transport TCP à threads, découverte
//! UDP des parties du réseau local. Aucune dépendance au jeu : tout se teste en boucle locale.
//!
//! Modèle : un joueur **héberge** (serveur intégré, voir `nb-game::net`), les autres se
//! **connectent** ; pas de serveur dédié. Le jeu garde la logique ; ce crate ne fait que
//! transporter des messages typés.
//!
//! * [`wire`] : lecture/écriture binaire (little-endian) ;
//! * [`proto`] : messages client → serveur ([`ClientMsg`]) et serveur → client ([`ServerMsg`]) ;
//! * [`transport`] : [`NetServer`] / [`NetClient`], trames TCP préfixées par leur longueur ;
//! * [`discovery`] : [`Beacon`] (l'hôte répond) et [`Scanner`] (le client cherche), UDP ;
//! * [`names`] : noms de joueur valides et uniques.

pub mod discovery;
pub mod names;
pub mod proto;
pub mod transport;
pub mod wire;

pub use discovery::{
    local_ipv4, Beacon, DiscoveredServer, Scanner, ServerInfo, DEFAULT_DISCOVERY_PORT,
};
pub use names::{sanitize_name, unique_name, MAX_NAME_LEN};
pub use proto::{
    ClientMsg, MobPose, PlayerPose, ServerMsg, Welcome, DEFAULT_GAME_PORT, MAX_PLAYERS,
    PROTOCOL_VERSION,
};
pub use transport::{ClientEvent, ConnId, NetClient, NetServer, ServerEvent};
