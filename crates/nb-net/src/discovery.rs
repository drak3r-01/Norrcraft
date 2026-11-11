//! Découverte des parties du réseau local, en UDP.
//!
//! * l'hôte lance un [`Beacon`] qui répond aux sondes ;
//! * le client lance un [`Scanner`] qui envoie une sonde en diffusion (`255.255.255.255`, le /24
//!   local et la boucle locale) toutes les 1,5 s et liste les réponses.
//!
//! Sans diffusion possible (réseau d'invités, VPN…), la saisie manuelle `adresse:port` reste
//! disponible dans le jeu.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::wire::{Reader, WireError, Writer};

/// Port UDP de découverte par défaut.
pub const DEFAULT_DISCOVERY_PORT: u16 = 25571;
const PROBE: &[u8; 6] = b"NBLAN?";
const REPLY: &[u8; 6] = b"NBLAN!";
/// Une partie non revue depuis ce délai disparaît de la liste.
pub const SERVER_TTL: Duration = Duration::from_secs(5);
/// Intervalle entre deux sondes.
pub const PROBE_INTERVAL: Duration = Duration::from_millis(1500);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ce qu'une partie annonce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    pub protocol: u16,
    /// Port TCP de jeu.
    pub port: u16,
    pub host_name: String,
    pub world_name: String,
    pub players: u8,
    pub max_players: u8,
    /// Univers du monde (`vanilla`, `norrcraft`) ; vide si l'hôte ne l'annonce pas.
    pub world_kind: String,
}

impl ServerInfo {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.buf.extend_from_slice(REPLY);
        w.u16(self.protocol)
            .u16(self.port)
            .str(&self.host_name)
            .str(&self.world_name)
            .u8(self.players)
            .u8(self.max_players)
            .str(&self.world_kind);
        w.buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, WireError> {
        if data.len() < REPLY.len() || &data[..REPLY.len()] != REPLY {
            return Err(WireError::BadTag(data.first().copied().unwrap_or(0)));
        }
        let mut r = Reader::new(&data[REPLY.len()..]);
        Ok(Self {
            protocol: r.u16()?,
            port: r.u16()?,
            host_name: r.str()?,
            world_name: r.str()?,
            players: r.u8()?,
            max_players: r.u8()?,
            world_kind: r.str().unwrap_or_default(),
        })
    }
}

/// Répondeur de l'hôte.
pub struct Beacon {
    info: Arc<Mutex<ServerInfo>>,
    stop: Arc<AtomicBool>,
    port: u16,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Beacon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Beacon").field("port", &self.port).finish()
    }
}

impl Beacon {
    /// Écoute les sondes sur `port` UDP. Échoue si le port est déjà pris (autre partie sur la
    /// même machine) : la partie reste jouable par adresse directe.
    pub fn start(port: u16, info: ServerInfo) -> io::Result<Self> {
        let sock = UdpSocket::bind(("0.0.0.0", port))?;
        sock.set_read_timeout(Some(Duration::from_millis(150)))?;
        let port = sock.local_addr()?.port();
        let info = Arc::new(Mutex::new(info));
        let stop = Arc::new(AtomicBool::new(false));
        let (i, s) = (info.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("nb-net-beacon".into())
            .spawn(move || {
                let mut buf = [0u8; 64];
                while !s.load(Ordering::Acquire) {
                    match sock.recv_from(&mut buf) {
                        Ok((n, src)) if n >= PROBE.len() && &buf[..PROBE.len()] == PROBE => {
                            let reply = lock(&i).encode();
                            let _ = sock.send_to(&reply, src);
                        }
                        _ => {}
                    }
                }
            })?;
        Ok(Self {
            info,
            stop,
            port,
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Met à jour l'annonce (nombre de joueurs…).
    pub fn set_info(&self, info: ServerInfo) {
        *lock(&self.info) = info;
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Une partie trouvée sur le réseau.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredServer {
    /// Adresse TCP à joindre.
    pub addr: SocketAddr,
    pub info: ServerInfo,
}

/// Chercheur de parties.
pub struct Scanner {
    sock: UdpSocket,
    port: u16,
    found: HashMap<SocketAddr, (ServerInfo, Instant)>,
    last_probe: Option<Instant>,
}

impl std::fmt::Debug for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scanner")
            .field("found", &self.found.len())
            .finish()
    }
}

impl Scanner {
    pub fn new() -> io::Result<Self> {
        Self::with_port(DEFAULT_DISCOVERY_PORT)
    }

    /// `port` : port UDP où les hôtes écoutent.
    pub fn with_port(port: u16) -> io::Result<Self> {
        let sock = UdpSocket::bind(("0.0.0.0", 0))?;
        sock.set_broadcast(true)?;
        sock.set_nonblocking(true)?;
        Ok(Self {
            sock,
            port,
            found: HashMap::new(),
            last_probe: None,
        })
    }

    /// Envoie une sonde maintenant.
    pub fn probe(&mut self) {
        let mut targets = vec![
            SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)),
            SocketAddr::from((Ipv4Addr::BROADCAST, self.port)),
        ];
        if let Some(ip) = local_ipv4() {
            let o = ip.octets();
            targets.push(SocketAddr::from((
                Ipv4Addr::new(o[0], o[1], o[2], 255),
                self.port,
            )));
        }
        for t in targets {
            let _ = self.sock.send_to(PROBE, t);
        }
        self.last_probe = Some(Instant::now());
    }

    /// À appeler à chaque tick : sonde périodique, lecture des réponses, oubli des absents.
    pub fn poll(&mut self) {
        if self
            .last_probe
            .is_none_or(|t| t.elapsed() >= PROBE_INTERVAL)
        {
            self.probe();
        }
        let mut buf = [0u8; 512];
        while let Ok((n, src)) = self.sock.recv_from(&mut buf) {
            if let Ok(info) = ServerInfo::decode(&buf[..n]) {
                let addr = SocketAddr::new(src.ip(), info.port);
                self.found.insert(addr, (info, Instant::now()));
            }
        }
        self.found
            .retain(|_, (_, seen)| seen.elapsed() < SERVER_TTL);
    }

    /// Parties vues récemment, triées par nom de monde puis adresse.
    pub fn servers(&self) -> Vec<DiscoveredServer> {
        let mut v: Vec<_> = self
            .found
            .iter()
            .map(|(addr, (info, _))| DiscoveredServer {
                addr: *addr,
                info: info.clone(),
            })
            .collect();
        v.sort_by(|a, b| {
            (&a.info.world_name, a.addr.to_string()).cmp(&(&b.info.world_name, b.addr.to_string()))
        });
        v
    }
}

/// Adresse IPv4 locale utilisée pour sortir sur le réseau (aucun paquet n'est envoyé : la
/// connexion UDP ne fait que choisir l'interface).
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    s.connect(("8.8.8.8", 80)).ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(players: u8) -> ServerInfo {
        ServerInfo {
            protocol: 1,
            port: 25570,
            host_name: "Gabriel".into(),
            world_name: "Mon monde".into(),
            players,
            max_players: 8,
            world_kind: "norrcraft".into(),
        }
    }

    #[test]
    fn info_round_trip_and_garbage() {
        let i = info(3);
        assert_eq!(ServerInfo::decode(&i.encode()).unwrap(), i);
        assert!(ServerInfo::decode(b"").is_err());
        assert!(ServerInfo::decode(b"NBLAN!").is_err());
        assert!(ServerInfo::decode(b"HTTP/1.1 200 OK").is_err());
    }

    #[test]
    fn scanner_finds_beacon_and_sees_updates() {
        let beacon = Beacon::start(0, info(1)).unwrap();
        let mut sc = Scanner::with_port(beacon.port()).unwrap();
        let t = Instant::now();
        let found = loop {
            sc.poll();
            if let Some(s) = sc.servers().into_iter().next() {
                break s;
            }
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "aucune partie trouvée"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(found.info, info(1));
        assert_eq!(found.addr.port(), 25570);
        beacon.set_info(info(4));
        sc.probe();
        let t = Instant::now();
        loop {
            sc.poll();
            if sc.servers().first().map(|s| s.info.players) == Some(4) {
                break;
            }
            assert!(
                t.elapsed() < Duration::from_secs(5),
                "annonce non mise à jour"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn beacon_ignores_noise() {
        let beacon = Beacon::start(0, info(1)).unwrap();
        let s = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(300)))
            .unwrap();
        s.send_to(b"hello", ("127.0.0.1", beacon.port())).unwrap();
        let mut buf = [0u8; 64];
        assert!(s.recv_from(&mut buf).is_err(), "pas de réponse au bruit");
    }

    #[test]
    fn stale_servers_expire() {
        let mut sc = Scanner::with_port(1).unwrap();
        sc.found.insert(
            "127.0.0.1:1".parse().unwrap(),
            (
                info(1),
                Instant::now() - SERVER_TTL - Duration::from_secs(1),
            ),
        );
        sc.poll();
        assert!(sc.servers().is_empty());
    }
}
