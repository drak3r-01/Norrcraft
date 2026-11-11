//! Recherche des parties du réseau local et lecture des adresses saisies.

use nb_net::{Scanner, DEFAULT_DISCOVERY_PORT, DEFAULT_GAME_PORT};

use super::ServerEntry;

/// Lit une adresse saisie : `192.168.1.20`, `192.168.1.20:25570`, `pc-de-lea:25570`, `[::1]:25570`.
/// Sans port, le port par défaut est ajouté. Renvoie `hôte:port` ou un message d'erreur.
pub fn parse_address(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("Type the host's address first (for example 192.168.1.20)".into());
    }
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The address cannot contain spaces".into());
    }
    let (host, port) = if let Some(rest) = t.strip_prefix('[') {
        // IPv6 entre crochets.
        let (h, tail) = rest.split_once(']').ok_or("Missing ']' in the address")?;
        match tail.strip_prefix(':') {
            Some(p) => (format!("[{h}]"), Some(p)),
            None if tail.is_empty() => (format!("[{h}]"), None),
            None => return Err("Unexpected text after ']'".into()),
        }
    } else {
        match t.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => (h.to_owned(), Some(p)),
            Some(_) => return Err("Write IPv6 addresses between [brackets]".into()),
            None => (t.to_owned(), None),
        }
    };
    if host.is_empty() {
        return Err("The host's address is missing".into());
    }
    let port = match port {
        Some(p) => p
            .parse::<u16>()
            .ok()
            .filter(|&p| p != 0)
            .ok_or_else(|| format!("'{p}' is not a valid port (1-65535)"))?,
        None => DEFAULT_GAME_PORT,
    };
    Ok(format!("{host}:{port}"))
}

/// Recherche active tant que l'écran « Multijoueur » est ouvert.
#[derive(Debug, Default)]
pub struct LanBrowser {
    scanner: Option<Scanner>,
    /// Message si la recherche est impossible (pas de réseau, port UDP occupé…).
    pub problem: Option<String>,
    discovery_port: Option<u16>,
}

impl LanBrowser {
    /// Port UDP de découverte particulier (tests : plusieurs jeux dans un processus).
    pub fn with_discovery_port(port: u16) -> Self {
        Self {
            discovery_port: Some(port),
            ..Self::default()
        }
    }

    pub fn start(&mut self) {
        if self.scanner.is_some() {
            return;
        }
        let port = self.discovery_port.unwrap_or(DEFAULT_DISCOVERY_PORT);
        match Scanner::with_port(port) {
            Ok(mut s) => {
                s.probe();
                self.scanner = Some(s);
                self.problem = None;
            }
            Err(e) => self.problem = Some(format!("Network search unavailable: {e}")),
        }
    }

    pub fn stop(&mut self) {
        self.scanner = None;
    }

    pub fn is_running(&self) -> bool {
        self.scanner.is_some()
    }

    /// Relance une sonde tout de suite (bouton « Refresh »).
    pub fn refresh(&mut self) {
        if let Some(s) = &mut self.scanner {
            s.probe();
        }
    }

    pub fn poll(&mut self) {
        if let Some(s) = &mut self.scanner {
            s.poll();
        }
    }

    pub fn servers(&self) -> Vec<ServerEntry> {
        self.scanner
            .as_ref()
            .map(|s| {
                s.servers()
                    .into_iter()
                    .filter(|d| d.info.protocol == nb_net::PROTOCOL_VERSION)
                    .map(|d| ServerEntry {
                        world: d.info.world_name,
                        host: d.info.host_name,
                        address: d.addr.to_string(),
                        players: d.info.players,
                        max_players: d.info.max_players,
                        world_kind: d.info.world_kind,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_get_the_default_port() {
        assert_eq!(parse_address("192.168.1.20").unwrap(), "192.168.1.20:25570");
        assert_eq!(
            parse_address("  192.168.1.20:4000 ").unwrap(),
            "192.168.1.20:4000"
        );
        assert_eq!(parse_address("pc-lea").unwrap(), "pc-lea:25570");
        assert_eq!(parse_address("[::1]").unwrap(), "[::1]:25570");
        assert_eq!(parse_address("[::1]:99").unwrap(), "[::1]:99");
    }

    #[test]
    fn bad_addresses_explain_themselves() {
        for bad in [
            "",
            "   ",
            "1.2.3.4:abc",
            "1.2.3.4:0",
            "1.2.3.4:70000",
            "::1",
            ":25570",
            "a b",
            "[::1",
        ] {
            assert!(parse_address(bad).is_err(), "{bad:?} devrait être refusée");
        }
        assert!(parse_address("1.2.3.4:abc").unwrap_err().contains("port"));
    }
}
