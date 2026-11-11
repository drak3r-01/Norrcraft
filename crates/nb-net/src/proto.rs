//! Messages du protocole. Chaque message est une trame `[étiquette u8][champs…]` ; le
//! transport ajoute la longueur. Version incrémentée à chaque changement incompatible.

use crate::wire::{Reader, WireError, Writer};

/// Version du protocole : refusée si elle diffère entre hôte et client.
///
/// 3 (plan Norrcraft, session 0.3) : `Hello` annonce le contenu du jeu de l'invité (empreinte de
/// la palette de blocs de chaque univers) ; `Welcome` donne l'univers et la palette de blocs du
/// monde de l'hôte.
pub const PROTOCOL_VERSION: u16 = 3;
/// Port TCP de jeu par défaut.
pub const DEFAULT_GAME_PORT: u16 = 25570;
/// Nombre maximal de joueurs (hôte compris).
pub const MAX_PLAYERS: usize = 8;

/// Pose d'un joueur dans un instantané.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerPose {
    pub id: u16,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
}

/// Pose d'un mob dans un instantané. `kind` indexe [`Welcome::mob_types`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MobPose {
    pub id: u32,
    pub kind: u16,
    pub pos: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub health: f32,
}

/// Données d'entrée dans le monde de l'hôte.
#[derive(Clone, Debug, PartialEq)]
pub struct Welcome {
    /// Identifiant réseau du joueur (0 = l'hôte).
    pub player_id: u16,
    /// Nom retenu (peut différer du nom demandé s'il était pris).
    pub name: String,
    pub host_name: String,
    pub world_name: String,
    pub seed: i64,
    /// `true` = créatif.
    pub creative: bool,
    pub allow_cheats: bool,
    /// `WorldConfig` de l'hôte (JSON).
    pub config_json: String,
    pub spawn: [f64; 3],
    pub time_of_day: f32,
    pub day_length: f32,
    /// Table des types de mobs (index → identifiant).
    pub mob_types: Vec<String>,
    /// Univers du monde de l'hôte (`vanilla`, `norrcraft`).
    pub world_kind: String,
    /// Palette de blocs du monde de l'hôte : nom de chaque identifiant (`""` = libre). Les
    /// chunks échangés sont en identifiants de cette palette.
    pub block_palette: Vec<String>,
}

/// Client → serveur.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello {
        version: u16,
        name: String,
        render_distance: u8,
        /// Contenu de la version du jeu de l'invité : `(univers, empreinte de la palette par
        /// défaut)` pour chaque univers qu'elle connaît. L'hôte refuse l'invité dont la version
        /// ne connaît pas son univers ou en a une palette différente.
        content: Vec<(String, u64)>,
    },
    /// Position (pieds), orientation et santé du joueur, envoyées ~20 fois par seconde.
    State {
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
        health: f32,
        creative: bool,
        /// Réduction des dégâts de mobs par l'armure portée (0..0,8).
        reduction: f32,
    },
    /// Le joueur a posé (`block` ≠ air) ou cassé un bloc.
    BlockEdit {
        pos: [i32; 3],
        block: u8,
    },
    /// Le joueur frappe le mob `entity` avec `damage` (arme tenue).
    Attack {
        entity: u32,
        damage: f32,
    },
    Chat(String),
    Ping(u32),
    Bye,
}

/// Serveur → client.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome(Box<Welcome>),
    Refuse(String),
    Pong(u32),
    PlayerJoined {
        id: u16,
        name: String,
    },
    PlayerLeft {
        id: u16,
    },
    /// Poses des joueurs (hôte compris, client destinataire exclu) et des mobs proches.
    Snapshot {
        players: Vec<PlayerPose>,
        mobs: Vec<MobPose>,
    },
    /// Chunk modifié complet (`nb-world::storage::encode_chunk`).
    Chunk {
        cx: i32,
        cz: i32,
        data: Vec<u8>,
    },
    BlockChange {
        pos: [i32; 3],
        block: u8,
    },
    /// Santé imposée par l'hôte (dégâts de mob).
    SetHealth(f32),
    Time {
        time_of_day: f32,
        day_length: f32,
    },
    /// Message de chat ; `from` vide = message système.
    Chat {
        from: String,
        text: String,
    },
    Kick(String),
    /// Le mob `mob` (identifiant de type) vient d'être tué par ce joueur : il tire son butin.
    MobKilled(String),
}

mod tag {
    pub const C_HELLO: u8 = 1;
    pub const C_STATE: u8 = 2;
    pub const C_BLOCK: u8 = 3;
    pub const C_ATTACK: u8 = 4;
    pub const C_CHAT: u8 = 5;
    pub const C_PING: u8 = 6;
    pub const C_BYE: u8 = 7;

    pub const S_WELCOME: u8 = 101;
    pub const S_REFUSE: u8 = 102;
    pub const S_PONG: u8 = 103;
    pub const S_JOINED: u8 = 104;
    pub const S_LEFT: u8 = 105;
    pub const S_SNAPSHOT: u8 = 106;
    pub const S_CHUNK: u8 = 107;
    pub const S_BLOCK: u8 = 108;
    pub const S_HEALTH: u8 = 109;
    pub const S_TIME: u8 = 110;
    pub const S_CHAT: u8 = 111;
    pub const S_KICK: u8 = 112;
    pub const S_MOB_KILLED: u8 = 113;
}

fn vec3_f64(r: &mut Reader<'_>) -> Result<[f64; 3], WireError> {
    Ok([r.f64()?, r.f64()?, r.f64()?])
}
fn vec3_f32(r: &mut Reader<'_>) -> Result<[f32; 3], WireError> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}

impl ClientMsg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ClientMsg::Hello {
                version,
                name,
                render_distance,
                content,
            } => {
                let mut w = Writer::new(tag::C_HELLO);
                w.u16(*version).str(name).u8(*render_distance);
                w.u32(content.len() as u32);
                for (kind, fp) in content {
                    w.str(kind).i64(*fp as i64);
                }
                w.buf
            }
            ClientMsg::State {
                pos,
                yaw,
                pitch,
                health,
                creative,
                reduction,
            } => {
                let mut w = Writer::new(tag::C_STATE);
                w.f64(pos[0]).f64(pos[1]).f64(pos[2]);
                w.f32(*yaw).f32(*pitch).f32(*health).bool(*creative).f32(*reduction);
                w.buf
            }
            ClientMsg::BlockEdit { pos, block } => {
                let mut w = Writer::new(tag::C_BLOCK);
                w.i32(pos[0]).i32(pos[1]).i32(pos[2]).u8(*block);
                w.buf
            }
            ClientMsg::Attack { entity, damage } => {
                let mut w = Writer::new(tag::C_ATTACK);
                w.u32(*entity).f32(*damage);
                w.buf
            }
            ClientMsg::Chat(t) => {
                let mut w = Writer::new(tag::C_CHAT);
                w.str(t);
                w.buf
            }
            ClientMsg::Ping(n) => {
                let mut w = Writer::new(tag::C_PING);
                w.u32(*n);
                w.buf
            }
            ClientMsg::Bye => vec![tag::C_BYE],
        }
    }

    pub fn decode(data: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(data);
        Ok(match r.u8()? {
            tag::C_HELLO => {
                let version = r.u16()?;
                let name = r.str()?;
                let render_distance = r.u8()?;
                let n = r.len(12)?;
                let mut content = Vec::with_capacity(n);
                for _ in 0..n {
                    content.push((r.str()?, r.i64()? as u64));
                }
                ClientMsg::Hello {
                    version,
                    name,
                    render_distance,
                    content,
                }
            }
            tag::C_STATE => ClientMsg::State {
                pos: vec3_f64(&mut r)?,
                yaw: r.f32()?,
                pitch: r.f32()?,
                health: r.f32()?,
                creative: r.bool()?,
                reduction: r.f32()?,
            },
            tag::C_BLOCK => ClientMsg::BlockEdit {
                pos: [r.i32()?, r.i32()?, r.i32()?],
                block: r.u8()?,
            },
            tag::C_ATTACK => ClientMsg::Attack {
                entity: r.u32()?,
                damage: r.f32()?,
            },
            tag::C_CHAT => ClientMsg::Chat(r.str()?),
            tag::C_PING => ClientMsg::Ping(r.u32()?),
            tag::C_BYE => ClientMsg::Bye,
            t => return Err(WireError::BadTag(t)),
        })
    }
}

impl ServerMsg {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            ServerMsg::Welcome(m) => {
                let mut w = Writer::new(tag::S_WELCOME);
                w.u16(m.player_id)
                    .str(&m.name)
                    .str(&m.host_name)
                    .str(&m.world_name)
                    .i64(m.seed)
                    .bool(m.creative)
                    .bool(m.allow_cheats)
                    .str(&m.config_json);
                w.f64(m.spawn[0]).f64(m.spawn[1]).f64(m.spawn[2]);
                w.f32(m.time_of_day).f32(m.day_length);
                w.u32(m.mob_types.len() as u32);
                for t in &m.mob_types {
                    w.str(t);
                }
                w.str(&m.world_kind);
                w.u32(m.block_palette.len() as u32);
                for n in &m.block_palette {
                    w.str(n);
                }
                w.buf
            }
            ServerMsg::Refuse(t) => {
                let mut w = Writer::new(tag::S_REFUSE);
                w.str(t);
                w.buf
            }
            ServerMsg::Pong(n) => {
                let mut w = Writer::new(tag::S_PONG);
                w.u32(*n);
                w.buf
            }
            ServerMsg::PlayerJoined { id, name } => {
                let mut w = Writer::new(tag::S_JOINED);
                w.u16(*id).str(name);
                w.buf
            }
            ServerMsg::PlayerLeft { id } => {
                let mut w = Writer::new(tag::S_LEFT);
                w.u16(*id);
                w.buf
            }
            ServerMsg::Snapshot { players, mobs } => {
                let mut w = Writer::new(tag::S_SNAPSHOT);
                w.u32(players.len() as u32);
                for p in players {
                    w.u16(p.id).f32(p.pos[0]).f32(p.pos[1]).f32(p.pos[2]);
                    w.f32(p.yaw).f32(p.pitch);
                }
                w.u32(mobs.len() as u32);
                for m in mobs {
                    w.u32(m.id).u16(m.kind);
                    w.f32(m.pos[0]).f32(m.pos[1]).f32(m.pos[2]);
                    w.f32(m.yaw).f32(m.pitch).f32(m.health);
                }
                w.buf
            }
            ServerMsg::Chunk { cx, cz, data } => {
                let mut w = Writer::new(tag::S_CHUNK);
                w.i32(*cx).i32(*cz).bytes(data);
                w.buf
            }
            ServerMsg::BlockChange { pos, block } => {
                let mut w = Writer::new(tag::S_BLOCK);
                w.i32(pos[0]).i32(pos[1]).i32(pos[2]).u8(*block);
                w.buf
            }
            ServerMsg::SetHealth(h) => {
                let mut w = Writer::new(tag::S_HEALTH);
                w.f32(*h);
                w.buf
            }
            ServerMsg::Time {
                time_of_day,
                day_length,
            } => {
                let mut w = Writer::new(tag::S_TIME);
                w.f32(*time_of_day).f32(*day_length);
                w.buf
            }
            ServerMsg::Chat { from, text } => {
                let mut w = Writer::new(tag::S_CHAT);
                w.str(from).str(text);
                w.buf
            }
            ServerMsg::Kick(t) => {
                let mut w = Writer::new(tag::S_KICK);
                w.str(t);
                w.buf
            }
            ServerMsg::MobKilled(m) => {
                let mut w = Writer::new(tag::S_MOB_KILLED);
                w.str(m);
                w.buf
            }
        }
    }

    pub fn decode(data: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(data);
        Ok(match r.u8()? {
            tag::S_WELCOME => {
                let player_id = r.u16()?;
                let name = r.str()?;
                let host_name = r.str()?;
                let world_name = r.str()?;
                let seed = r.i64()?;
                let creative = r.bool()?;
                let allow_cheats = r.bool()?;
                let config_json = r.str()?;
                let spawn = vec3_f64(&mut r)?;
                let time_of_day = r.f32()?;
                let day_length = r.f32()?;
                let n = r.len(4)?;
                let mut mob_types = Vec::with_capacity(n);
                for _ in 0..n {
                    mob_types.push(r.str()?);
                }
                let world_kind = r.str()?;
                let n = r.len(4)?;
                let mut block_palette = Vec::with_capacity(n);
                for _ in 0..n {
                    block_palette.push(r.str()?);
                }
                ServerMsg::Welcome(Box::new(Welcome {
                    player_id,
                    name,
                    host_name,
                    world_name,
                    seed,
                    creative,
                    allow_cheats,
                    config_json,
                    spawn,
                    time_of_day,
                    day_length,
                    mob_types,
                    world_kind,
                    block_palette,
                }))
            }
            tag::S_REFUSE => ServerMsg::Refuse(r.str()?),
            tag::S_PONG => ServerMsg::Pong(r.u32()?),
            tag::S_JOINED => ServerMsg::PlayerJoined {
                id: r.u16()?,
                name: r.str()?,
            },
            tag::S_LEFT => ServerMsg::PlayerLeft { id: r.u16()? },
            tag::S_SNAPSHOT => {
                let n = r.len(22)?;
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push(PlayerPose {
                        id: r.u16()?,
                        pos: vec3_f32(&mut r)?,
                        yaw: r.f32()?,
                        pitch: r.f32()?,
                    });
                }
                let n = r.len(30)?;
                let mut mobs = Vec::with_capacity(n);
                for _ in 0..n {
                    mobs.push(MobPose {
                        id: r.u32()?,
                        kind: r.u16()?,
                        pos: vec3_f32(&mut r)?,
                        yaw: r.f32()?,
                        pitch: r.f32()?,
                        health: r.f32()?,
                    });
                }
                ServerMsg::Snapshot { players, mobs }
            }
            tag::S_CHUNK => ServerMsg::Chunk {
                cx: r.i32()?,
                cz: r.i32()?,
                data: r.bytes()?,
            },
            tag::S_BLOCK => ServerMsg::BlockChange {
                pos: [r.i32()?, r.i32()?, r.i32()?],
                block: r.u8()?,
            },
            tag::S_HEALTH => ServerMsg::SetHealth(r.f32()?),
            tag::S_TIME => ServerMsg::Time {
                time_of_day: r.f32()?,
                day_length: r.f32()?,
            },
            tag::S_CHAT => ServerMsg::Chat {
                from: r.str()?,
                text: r.str()?,
            },
            tag::S_KICK => ServerMsg::Kick(r.str()?),
            tag::S_MOB_KILLED => ServerMsg::MobKilled(r.str()?),
            t => return Err(WireError::BadTag(t)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn welcome() -> Welcome {
        Welcome {
            player_id: 3,
            name: "Léa".into(),
            host_name: "Gabriel".into(),
            world_name: "Mon monde".into(),
            seed: -42,
            creative: true,
            allow_cheats: false,
            config_json: "{\"seaLevel\":62}".into(),
            spawn: [1.5, 70.0, -3.25],
            time_of_day: 0.25,
            day_length: 600.0,
            mob_types: vec!["wild_boar".into(), "elk".into()],
            world_kind: "norrcraft".into(),
            block_palette: vec!["air".into(), "granit".into(), String::new(), "grus".into()],
        }
    }

    #[test]
    fn client_messages_round_trip() {
        let msgs = [
            ClientMsg::Hello {
                version: PROTOCOL_VERSION,
                name: "Gab".into(),
                render_distance: 8,
                content: vec![("vanilla".into(), u64::MAX - 7), ("norrcraft".into(), 42)],
            },
            ClientMsg::State {
                pos: [1.0, 2.0, 3.0],
                yaw: 0.5,
                pitch: -0.25,
                health: 7.5,
                creative: false,
                reduction: 0.45,
            },
            ClientMsg::BlockEdit {
                pos: [-5, 64, 1_000_000],
                block: 12,
            },
            ClientMsg::Attack {
                entity: 99,
                damage: 5.5,
            },
            ClientMsg::Chat("salut à tous".into()),
            ClientMsg::Ping(5),
            ClientMsg::Bye,
        ];
        for m in msgs {
            assert_eq!(ClientMsg::decode(&m.encode()).unwrap(), m);
        }
    }

    #[test]
    fn server_messages_round_trip() {
        let msgs = [
            ServerMsg::Welcome(Box::new(welcome())),
            ServerMsg::Refuse("Partie pleine".into()),
            ServerMsg::Pong(1),
            ServerMsg::PlayerJoined {
                id: 2,
                name: "Léa".into(),
            },
            ServerMsg::PlayerLeft { id: 2 },
            ServerMsg::Snapshot {
                players: vec![PlayerPose {
                    id: 0,
                    pos: [1.0, 2.0, 3.0],
                    yaw: 1.0,
                    pitch: 0.0,
                }],
                mobs: vec![
                    MobPose {
                        id: 10,
                        kind: 1,
                        pos: [4.0, 5.0, 6.0],
                        yaw: 2.0,
                        pitch: 0.1,
                        health: 3.0,
                    };
                    3
                ],
            },
            ServerMsg::Chunk {
                cx: -1,
                cz: 4,
                data: vec![1, 2, 3, 4],
            },
            ServerMsg::BlockChange {
                pos: [1, 2, 3],
                block: 0,
            },
            ServerMsg::SetHealth(2.0),
            ServerMsg::Time {
                time_of_day: 0.5,
                day_length: 300.0,
            },
            ServerMsg::Chat {
                from: "Gab".into(),
                text: "yo".into(),
            },
            ServerMsg::Kick("fin de partie".into()),
            ServerMsg::MobKilled("grey_wolf".into()),
        ];
        for m in msgs {
            assert_eq!(ServerMsg::decode(&m.encode()).unwrap(), m);
        }
    }

    #[test]
    fn garbage_never_panics() {
        assert!(ClientMsg::decode(&[]).is_err());
        assert!(ClientMsg::decode(&[250]).is_err());
        assert!(ServerMsg::decode(&[106, 255, 255, 255, 255]).is_err());
        let full = ServerMsg::Welcome(Box::new(welcome())).encode();
        for cut in 0..full.len() {
            assert!(ServerMsg::decode(&full[..cut]).is_err(), "coupe à {cut}");
        }
        // Octets pseudo-aléatoires : jamais de panique.
        let mut x = 12345u32;
        for _ in 0..2000 {
            let n = (x % 64) as usize;
            let buf: Vec<u8> = (0..n)
                .map(|_| {
                    x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (x >> 24) as u8
                })
                .collect();
            let _ = ClientMsg::decode(&buf);
            let _ = ServerMsg::decode(&buf);
        }
    }
}
