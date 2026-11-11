//! Transport TCP : trames `[longueur u32 LE][message]`, un thread de lecture et un thread
//! d'écriture par connexion, événements relevés sans bloquer par `poll()` (appelé à chaque tick).
//!
//! TCP suffit sur un réseau local : les instantanés (20 Hz) sont petits et un paquet perdu est
//! sans effet visible. `TCP_NODELAY` évite le groupement des petits messages.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::proto::{ClientMsg, ServerMsg};

/// Taille maximale d'une trame (un chunk compressé tient en quelques Ko ; marge large).
pub const MAX_FRAME: usize = 8 << 20;
/// Octets en attente d'écriture au-delà desquels un pair trop lent est déconnecté.
pub const MAX_QUEUED_BYTES: usize = 48 << 20;
/// Délai de connexion TCP.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Identifiant de connexion côté serveur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnId(pub u32);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn write_frame(w: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

fn read_frame(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len)?;
    let n = u32::from_le_bytes(len) as usize;
    if n == 0 || n > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("trame de {n} octets refusée"),
        ));
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

type Frame = Arc<Vec<u8>>;

/// Événement côté serveur.
#[derive(Debug, PartialEq)]
pub enum ServerEvent {
    Connected { conn: ConnId, addr: SocketAddr },
    Message { conn: ConnId, msg: ClientMsg },
    Disconnected { conn: ConnId, reason: String },
}

struct Peer {
    tx: Sender<Frame>,
    stream: TcpStream,
    queued: Arc<AtomicUsize>,
}

impl Peer {
    fn close(&self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

struct ServerShared {
    peers: Mutex<HashMap<ConnId, Peer>>,
    events: Sender<ServerEvent>,
    shutdown: AtomicBool,
}

/// Serveur TCP intégré : accepte les connexions et relaie les messages.
pub struct NetServer {
    shared: Arc<ServerShared>,
    events_rx: Receiver<ServerEvent>,
    port: u16,
    accept: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for NetServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetServer")
            .field("port", &self.port)
            .field("peers", &lock(&self.shared.peers).len())
            .finish()
    }
}

impl NetServer {
    /// Écoute sur `0.0.0.0:port` (`0` = port libre choisi par le système).
    pub fn bind(port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let (events, events_rx) = unbounded();
        let shared = Arc::new(ServerShared {
            peers: Mutex::new(HashMap::new()),
            events,
            shutdown: AtomicBool::new(false),
        });
        let s = shared.clone();
        let accept = std::thread::Builder::new()
            .name("nb-net-accept".into())
            .spawn(move || accept_loop(listener, s))?;
        Ok(Self {
            shared,
            events_rx,
            port,
            accept: Some(accept),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Événements arrivés depuis le dernier appel.
    pub fn poll(&self) -> Vec<ServerEvent> {
        self.events_rx.try_iter().collect()
    }

    pub fn connection_count(&self) -> usize {
        lock(&self.shared.peers).len()
    }

    /// Envoie un message à une connexion (ignoré si elle n'existe plus).
    pub fn send(&self, conn: ConnId, msg: &ServerMsg) {
        self.send_encoded(conn, Arc::new(msg.encode()));
    }

    /// Envoie un message déjà encodé (diffusion : un seul encodage pour tous).
    pub fn send_encoded(&self, conn: ConnId, frame: Frame) {
        let too_slow = {
            let peers = lock(&self.shared.peers);
            let Some(p) = peers.get(&conn) else {
                return;
            };
            let q = p.queued.fetch_add(frame.len(), Ordering::AcqRel) + frame.len();
            let _ = p.tx.send(frame);
            q > MAX_QUEUED_BYTES
        };
        if too_slow {
            self.drop_peer(conn, "connexion trop lente".into());
        }
    }

    /// Envoie à toutes les connexions.
    pub fn broadcast(&self, msg: &ServerMsg) {
        let frame = Arc::new(msg.encode());
        let ids: Vec<ConnId> = lock(&self.shared.peers).keys().copied().collect();
        for id in ids {
            self.send_encoded(id, frame.clone());
        }
    }

    /// Ferme une connexion (aucun événement `Disconnected` n'est émis pour elle).
    pub fn disconnect(&self, conn: ConnId) {
        if let Some(p) = lock(&self.shared.peers).remove(&conn) {
            // Laisse partir les messages en file (ex. `Kick`) avant la fermeture.
            let stream = p.stream.try_clone();
            drop(p.tx);
            if let Ok(s) = stream {
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    let _ = s.shutdown(Shutdown::Both);
                });
            }
        }
    }

    fn drop_peer(&self, conn: ConnId, reason: String) {
        let removed = lock(&self.shared.peers).remove(&conn);
        if let Some(p) = removed {
            p.close();
            let _ = self
                .shared
                .events
                .send(ServerEvent::Disconnected { conn, reason });
        }
    }

    /// Arrête l'écoute et ferme toutes les connexions.
    pub fn shutdown(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        let peers: Vec<Peer> = lock(&self.shared.peers).drain().map(|(_, p)| p).collect();
        for p in peers {
            p.close();
        }
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
    }
}

impl Drop for NetServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<ServerShared>) {
    let mut next = 1u32;
    while !shared.shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, addr)) => {
                let id = ConnId(next);
                next = next.wrapping_add(1);
                if let Err(e) = start_peer(id, stream, addr, &shared) {
                    eprintln!("réseau : connexion de {addr} ignorée : {e}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(e) => {
                eprintln!("réseau : accept : {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn start_peer(
    id: ConnId,
    stream: TcpStream,
    addr: SocketAddr,
    shared: &Arc<ServerShared>,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    let (tx, rx) = unbounded::<Frame>();
    let queued = Arc::new(AtomicUsize::new(0));
    let mut wstream = stream.try_clone()?;
    let rstream = stream.try_clone()?;
    lock(&shared.peers).insert(
        id,
        Peer {
            tx,
            stream,
            queued: queued.clone(),
        },
    );
    let _ = shared
        .events
        .send(ServerEvent::Connected { conn: id, addr });

    let q = queued;
    std::thread::Builder::new()
        .name(format!("nb-net-w{}", id.0))
        .spawn(move || {
            for frame in rx.iter() {
                q.fetch_sub(frame.len().min(q.load(Ordering::Acquire)), Ordering::AcqRel);
                if write_frame(&mut wstream, &frame).is_err() {
                    break;
                }
            }
            // Canal fermé (déconnexion volontaire) ou erreur d'écriture : on coupe le socket.
            let _ = wstream.shutdown(Shutdown::Both);
        })?;

    let sh = shared.clone();
    std::thread::Builder::new()
        .name(format!("nb-net-r{}", id.0))
        .spawn(move || {
            let mut r = rstream;
            let reason = loop {
                match read_frame(&mut r) {
                    Ok(buf) => match ClientMsg::decode(&buf) {
                        Ok(msg) => {
                            let _ = sh.events.send(ServerEvent::Message { conn: id, msg });
                        }
                        Err(e) => break format!("message invalide : {e}"),
                    },
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        break "connexion fermée".to_owned()
                    }
                    Err(e) => break e.to_string(),
                }
            };
            let removed = lock(&sh.peers).remove(&id);
            if let Some(p) = removed {
                p.close();
                let _ = sh
                    .events
                    .send(ServerEvent::Disconnected { conn: id, reason });
            }
        })?;
    Ok(())
}

/// Événement côté client.
#[derive(Debug, PartialEq)]
pub enum ClientEvent {
    Connected,
    Message(ServerMsg),
    /// Connexion perdue ou impossible (le texte est montré au joueur).
    Disconnected(String),
}

/// Connexion sortante vers un hôte.
pub struct NetClient {
    tx: Option<Sender<Frame>>,
    events_rx: Receiver<ClientEvent>,
    stream: Arc<Mutex<Option<TcpStream>>>,
    closed: Arc<AtomicBool>,
}

impl std::fmt::Debug for NetClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetClient").finish_non_exhaustive()
    }
}

impl NetClient {
    /// Lance la connexion en arrière-plan (l'interface ne gèle pas) ; le résultat arrive par
    /// `poll()` : [`ClientEvent::Connected`] ou [`ClientEvent::Disconnected`].
    pub fn connect(addr: impl ToSocketAddrs + Send + 'static) -> Self {
        let (tx, rx) = unbounded::<Frame>();
        let (ev, events_rx) = unbounded();
        let stream: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let closed = Arc::new(AtomicBool::new(false));
        let (st, cl) = (stream.clone(), closed.clone());
        let spawned = std::thread::Builder::new()
            .name("nb-net-client".into())
            .spawn({
                let ev = ev.clone();
                move || client_main(addr, rx, ev, st, cl)
            });
        if let Err(e) = spawned {
            let _ = ev.send(ClientEvent::Disconnected(format!("thread réseau : {e}")));
        }
        Self {
            tx: Some(tx),
            events_rx,
            stream,
            closed,
        }
    }

    pub fn poll(&self) -> Vec<ClientEvent> {
        self.events_rx.try_iter().collect()
    }

    pub fn send(&self, msg: &ClientMsg) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Arc::new(msg.encode()));
        }
    }

    /// Ferme la connexion (un `Bye` est envoyé en dernier, best effort).
    pub fn close(&mut self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.send(&ClientMsg::Bye);
        // Fermer le canal laisse le thread d'écriture vider la file puis couper.
        self.tx = None;
        let stream = self.stream.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(s) = lock(&stream).take() {
                let _ = s.shutdown(Shutdown::Both);
            }
        });
    }
}

impl Drop for NetClient {
    fn drop(&mut self) {
        self.close();
    }
}

fn client_main(
    addr: impl ToSocketAddrs,
    rx: Receiver<Frame>,
    ev: Sender<ClientEvent>,
    shared_stream: Arc<Mutex<Option<TcpStream>>>,
    closed: Arc<AtomicBool>,
) {
    let fail = |msg: String| {
        let _ = ev.send(ClientEvent::Disconnected(msg));
    };
    let addrs: Vec<SocketAddr> = match addr.to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(e) => return fail(format!("adresse invalide : {e}")),
    };
    let mut last_err = None;
    let mut stream = None;
    for a in addrs {
        match TcpStream::connect_timeout(&a, CONNECT_TIMEOUT) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    let Some(stream) = stream else {
        return fail(match last_err {
            Some(e) => format!("connexion impossible : {e}"),
            None => "adresse introuvable".into(),
        });
    };
    if closed.load(Ordering::Acquire) {
        return;
    }
    let _ = stream.set_nodelay(true);
    let (Ok(mut wstream), Ok(rstream), Ok(keep)) =
        (stream.try_clone(), stream.try_clone(), stream.try_clone())
    else {
        return fail("socket inutilisable".into());
    };
    *lock(&shared_stream) = Some(keep);
    let _ = ev.send(ClientEvent::Connected);

    let ev2 = ev.clone();
    let cl = closed.clone();
    let reader = std::thread::Builder::new()
        .name("nb-net-client-r".into())
        .spawn(move || {
            let mut r = rstream;
            let reason = loop {
                match read_frame(&mut r) {
                    Ok(buf) => match ServerMsg::decode(&buf) {
                        Ok(m) => {
                            let _ = ev2.send(ClientEvent::Message(m));
                        }
                        Err(e) => break format!("message invalide : {e}"),
                    },
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                        break "connexion fermée par l'hôte".to_owned()
                    }
                    Err(e) => break e.to_string(),
                }
            };
            if !cl.load(Ordering::Acquire) {
                let _ = ev2.send(ClientEvent::Disconnected(reason));
            }
        });
    if reader.is_err() {
        return fail("thread de lecture".into());
    }
    for frame in rx.iter() {
        if write_frame(&mut wstream, &frame).is_err() {
            break;
        }
    }
    let _ = wstream.shutdown(Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn wait<T>(mut f: impl FnMut() -> Option<T>) -> T {
        let t = Instant::now();
        loop {
            if let Some(v) = f() {
                return v;
            }
            assert!(t.elapsed() < Duration::from_secs(5), "délai dépassé");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn local(server: &NetServer) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], server.port()))
    }

    #[test]
    fn hello_and_welcome() {
        let server = NetServer::bind(0).unwrap();
        let client = NetClient::connect(local(&server));
        wait(|| {
            client
                .poll()
                .into_iter()
                .find(|e| *e == ClientEvent::Connected)
        });
        client.send(&ClientMsg::Hello {
            version: 1,
            name: "Léa".into(),
            render_distance: 6,
            content: Vec::new(),
        });
        let conn = wait(|| {
            server.poll().into_iter().find_map(|e| match e {
                ServerEvent::Message { conn, msg } => {
                    assert!(matches!(msg, ClientMsg::Hello { .. }));
                    Some(conn)
                }
                _ => None,
            })
        });
        server.send(conn, &ServerMsg::Refuse("complet".into()));
        let m = wait(|| {
            client.poll().into_iter().find_map(|e| match e {
                ClientEvent::Message(m) => Some(m),
                _ => None,
            })
        });
        assert_eq!(m, ServerMsg::Refuse("complet".into()));
    }

    #[test]
    fn broadcast_and_disconnect_events() {
        let server = NetServer::bind(0).unwrap();
        let a = NetClient::connect(local(&server));
        let b = NetClient::connect(local(&server));
        wait(|| (server.connection_count() == 2).then_some(()));
        server.broadcast(&ServerMsg::Pong(7));
        for c in [&a, &b] {
            let got = wait(|| {
                c.poll().into_iter().find_map(|e| match e {
                    ClientEvent::Message(ServerMsg::Pong(n)) => Some(n),
                    _ => None,
                })
            });
            assert_eq!(got, 7);
        }
        drop(a);
        let reason = wait(|| {
            server.poll().into_iter().find_map(|e| match e {
                ServerEvent::Disconnected { reason, .. } => Some(reason),
                _ => None,
            })
        });
        assert!(!reason.is_empty());
        wait(|| (server.connection_count() == 1).then_some(()));
    }

    #[test]
    fn many_messages_stay_ordered() {
        let server = NetServer::bind(0).unwrap();
        let client = NetClient::connect(local(&server));
        for i in 0..500u32 {
            client.send(&ClientMsg::Ping(i));
        }
        let mut seen = Vec::new();
        wait(|| {
            for e in server.poll() {
                if let ServerEvent::Message {
                    msg: ClientMsg::Ping(n),
                    ..
                } = e
                {
                    seen.push(n);
                }
            }
            (seen.len() == 500).then_some(())
        });
        assert!(seen.windows(2).all(|w| w[0] + 1 == w[1]));
    }

    #[test]
    fn large_chunk_message() {
        let server = NetServer::bind(0).unwrap();
        let client = NetClient::connect(local(&server));
        let conn = wait(|| {
            server.poll().into_iter().find_map(|e| match e {
                ServerEvent::Connected { conn, .. } => Some(conn),
                _ => None,
            })
        });
        let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        server.send(
            conn,
            &ServerMsg::Chunk {
                cx: 1,
                cz: -2,
                data: data.clone(),
            },
        );
        let m = wait(|| {
            client.poll().into_iter().find_map(|e| match e {
                ClientEvent::Message(m) => Some(m),
                _ => None,
            })
        });
        assert_eq!(
            m,
            ServerMsg::Chunk {
                cx: 1,
                cz: -2,
                data
            }
        );
    }

    #[test]
    fn server_disconnect_is_reported_to_client() {
        let server = NetServer::bind(0).unwrap();
        let client = NetClient::connect(local(&server));
        let conn = wait(|| {
            server.poll().into_iter().find_map(|e| match e {
                ServerEvent::Connected { conn, .. } => Some(conn),
                _ => None,
            })
        });
        server.send(conn, &ServerMsg::Kick("au revoir".into()));
        server.disconnect(conn);
        let mut kicked = false;
        let reason = wait(|| {
            for e in client.poll() {
                match e {
                    ClientEvent::Message(ServerMsg::Kick(_)) => kicked = true,
                    ClientEvent::Disconnected(r) => return Some(r),
                    _ => {}
                }
            }
            None
        });
        assert!(kicked, "le Kick doit arriver avant la coupure");
        assert!(!reason.is_empty());
    }

    #[test]
    fn connection_refused_is_an_event() {
        // Port fermé : on réserve un port puis on le libère.
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let client = NetClient::connect(SocketAddr::from(([127, 0, 0, 1], port)));
        let reason = wait(|| {
            client.poll().into_iter().find_map(|e| match e {
                ClientEvent::Disconnected(r) => Some(r),
                _ => None,
            })
        });
        assert!(reason.contains("impossible"), "{reason}");
    }

    #[test]
    fn garbage_frame_disconnects_peer() {
        let server = NetServer::bind(0).unwrap();
        let mut raw = TcpStream::connect(local(&server)).unwrap();
        raw.write_all(&[3, 0, 0, 0, 250, 1, 2]).unwrap();
        let reason = wait(|| {
            server.poll().into_iter().find_map(|e| match e {
                ServerEvent::Disconnected { reason, .. } => Some(reason),
                _ => None,
            })
        });
        assert!(reason.contains("invalide"), "{reason}");
        // Longueur excessive.
        let mut raw = TcpStream::connect(local(&server)).unwrap();
        raw.write_all(&u32::MAX.to_le_bytes()).unwrap();
        wait(|| {
            server
                .poll()
                .into_iter()
                .find(|e| matches!(e, ServerEvent::Disconnected { .. }))
        });
    }
}
