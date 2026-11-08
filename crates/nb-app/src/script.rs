//! Mode scripté (`--script <fichier>`) : entrées simulées frame par frame, captures PNG,
//! vérifications d'état. Sert au scénario de bout en bout sous Docker (Xvfb + lavapipe) ; un
//! tick fixe par frame pour un déroulé reproductible. Outil de test, pas une fonctionnalité de
//! jeu.
//!
//! Une commande par ligne (`#` = commentaire) :
//! - `wait N` : N frames sans entrée ;
//! - `move X Y` : curseur en (X, Y) ; `look DX DY` : déplacement relatif (souris capturée) ;
//! - `click [X Y]` : clic gauche (appui à cette frame, relâchement à la suivante), sans
//!   coordonnées = curseur inchangé (en jeu, déplacer le curseur tourne la caméra) ;
//! - `rclick [X Y]` : idem, bouton droit ;
//! - `key K` : appui puis relâchement de la touche K (nom ou code GLFW) ;
//! - `hold K N` : touche K enfoncée pendant N frames ;
//! - `type TEXTE` : caractères saisis (après une frame d'attente) ;
//! - `scroll Y` : molette ;
//! - `capture NOM` : image de la frame suivante enregistrée sous `NOM.png` (hors écran) ;
//! - `expect-state ÉTAT` : échec si l'état du jeu diffère (`MainMenu`, `Playing`…) ;
//! - `wait-meshes N MAX` : attend au plus MAX frames qu'au moins N chunks soient maillés ;
//! - `spawn-mob ID DIST` : (outil) crée le mob ID à DIST blocs devant la caméra ;
//! - `spawn-particles ID N` : (outil) N particules ID devant la caméra ;
//! - `give OBJET N` : (outil) ajoute N exemplaires de l'objet à l'inventaire ;
//! - `vitals FAIM CHALEUR ENDURANCE` : (outil) fixe la faim (0-20), la chaleur et l'endurance ;
//! - `third-person` : (outil) bascule la vue à la 3e personne ;
//! - `quit`.

use nb_core::glam::Vec3;
use nb_core::JavaRandom;
use nb_data::keys;
use nb_game::input::glfw;
use nb_game::{Game, GameState};

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Wait(u32),
    Move(f64, f64),
    Look(f64, f64),
    /// Bouton, position (`None` = curseur inchangé : souris capturée en jeu).
    Click(usize, Option<(f64, f64)>),
    Key(i32),
    Hold(i32, u32),
    Type(String),
    Scroll(f64),
    Capture(String),
    ExpectState(GameState),
    WaitMeshes(usize, u32),
    SpawnMob(String, f32),
    SpawnParticles(String, u32),
    Give(String, i32),
    Vitals(f32, f32, f32),
    ThirdPerson,
    Quit,
}

/// Code GLFW d'un nom de touche (`ESC`, `ENTER`, `E`, `F3`…) ou d'un nombre.
pub fn key_code(name: &str) -> Option<i32> {
    if let Ok(n) = name.parse() {
        return Some(n);
    }
    let up = name.to_ascii_uppercase();
    Some(match up.as_str() {
        "ESC" | "ESCAPE" => keys::KEY_ESCAPE,
        "ENTER" | "RETURN" => keys::KEY_ENTER,
        "TAB" => keys::KEY_TAB,
        "BACKSPACE" => keys::KEY_BACKSPACE,
        "SPACE" => keys::KEY_SPACE,
        "SLASH" => keys::KEY_SLASH,
        "SHIFT" => 340,
        "CTRL" => 341,
        _ if up.len() == 1 && up.as_bytes()[0].is_ascii_alphanumeric() => {
            i32::from(up.as_bytes()[0])
        }
        _ if up.starts_with('F') => keys::KEY_F1 - 1 + up[1..].parse::<i32>().ok()?,
        _ => return None,
    })
}

fn state(name: &str) -> Option<GameState> {
    GameState::ALL
        .into_iter()
        .find(|s| format!("{s:?}").eq_ignore_ascii_case(name))
}

/// Analyse un script ; erreur = `ligne : message`.
pub fn parse(text: &str) -> Result<Vec<Cmd>, String> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let args: Vec<&str> = rest.split_whitespace().collect();
        let err = |m: &str| format!("ligne {} : {m} ({line})", n + 1);
        let num = |i: usize| -> Result<f64, String> {
            args.get(i)
                .and_then(|a| a.parse::<f64>().ok())
                .ok_or_else(|| err("nombre attendu"))
        };
        let key = |i: usize| -> Result<i32, String> {
            args.get(i)
                .and_then(|a| key_code(a))
                .ok_or_else(|| err("touche inconnue"))
        };
        let pos = || -> Result<Option<(f64, f64)>, String> {
            if args.is_empty() {
                Ok(None)
            } else {
                Ok(Some((num(0)?, num(1)?)))
            }
        };
        out.push(match word {
            "wait" => Cmd::Wait(num(0)? as u32),
            "move" => Cmd::Move(num(0)?, num(1)?),
            "look" => Cmd::Look(num(0)?, num(1)?),
            "click" => Cmd::Click(glfw::MOUSE_BUTTON_LEFT, pos()?),
            "rclick" => Cmd::Click(glfw::MOUSE_BUTTON_RIGHT, pos()?),
            "key" => Cmd::Key(key(0)?),
            "hold" => Cmd::Hold(key(0)?, num(1)? as u32),
            "type" => Cmd::Type(rest.to_owned()),
            "scroll" => Cmd::Scroll(num(0)?),
            "capture" => Cmd::Capture(args.first().ok_or_else(|| err("nom attendu"))?.to_string()),
            "expect-state" => Cmd::ExpectState(
                args.first()
                    .and_then(|a| state(a))
                    .ok_or_else(|| err("état inconnu"))?,
            ),
            "wait-meshes" => Cmd::WaitMeshes(num(0)? as usize, num(1)? as u32),
            "spawn-mob" => Cmd::SpawnMob(
                args.first().ok_or_else(|| err("id attendu"))?.to_string(),
                num(1)? as f32,
            ),
            "spawn-particles" => Cmd::SpawnParticles(
                args.first().ok_or_else(|| err("id attendu"))?.to_string(),
                num(1)? as u32,
            ),
            "give" => Cmd::Give(
                args.first().ok_or_else(|| err("objet attendu"))?.to_string(),
                num(1)? as i32,
            ),
            "vitals" => Cmd::Vitals(num(0)? as f32, num(1)? as f32, num(2)? as f32),
            "third-person" => Cmd::ThirdPerson,
            "quit" => Cmd::Quit,
            _ => return Err(err("commande inconnue")),
        });
    }
    Ok(out)
}

/// Ce que la frame doit faire après le rendu.
#[derive(Debug, Default, PartialEq)]
pub struct FrameOut {
    pub capture: Option<String>,
    pub quit: bool,
}

/// Exécution d'un script, une frame à la fois.
#[derive(Debug)]
pub struct Runner {
    cmds: Vec<Cmd>,
    pc: usize,
    wait: u32,
    /// (frames restantes, action de relâchement).
    releases: Vec<(u32, Release)>,
    rng: JavaRandom,
    pub failures: Vec<String>,
    pub frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Release {
    Key(i32),
    Button(usize),
}

impl Runner {
    pub fn new(cmds: Vec<Cmd>) -> Self {
        Self {
            cmds,
            pc: 0,
            wait: 0,
            releases: Vec::new(),
            rng: JavaRandom::new(42),
            failures: Vec::new(),
            frame: 0,
        }
    }

    pub fn is_done(&self) -> bool {
        self.pc >= self.cmds.len() && self.releases.is_empty() && self.wait == 0
    }

    fn fail(&mut self, msg: String) {
        eprintln!("SCRIPT ÉCHEC (frame {}) : {msg}", self.frame);
        self.failures.push(msg);
    }

    /// Avant le tick de la frame : relâchements dus, puis commandes jusqu'à la prochaine
    /// attente. `meshes` = nombre de chunks maillés côté GPU.
    pub fn before_frame(&mut self, g: &mut Game, meshes: usize) -> FrameOut {
        self.frame += 1;
        let mut out = FrameOut {
            capture: None,
            quit: false,
        };
        let mut due = Vec::new();
        self.releases.retain_mut(|(n, r)| {
            if *n == 0 {
                due.push(*r);
                false
            } else {
                *n -= 1;
                true
            }
        });
        for r in due {
            match r {
                Release::Key(k) => g.input.key_event(k, false, false),
                Release::Button(b) => g.input.mouse_button(b, false),
            }
        }
        if self.wait > 0 {
            self.wait -= 1;
            return out;
        }
        while let Some(cmd) = self.cmds.get(self.pc).cloned() {
            self.pc += 1;
            match cmd {
                Cmd::Wait(n) => {
                    self.wait = n.saturating_sub(1);
                    return out;
                }
                Cmd::Move(x, y) => g.input.cursor_moved(x, y),
                Cmd::Look(dx, dy) => {
                    let (x, y) = g.input.cursor();
                    g.input.cursor_moved(x + dx, y + dy);
                }
                Cmd::Click(b, pos) => {
                    if let Some((x, y)) = pos {
                        g.input.cursor_moved(x, y);
                    }
                    g.input.mouse_button(b, true);
                    self.releases.push((0, Release::Button(b)));
                    return out;
                }
                Cmd::Key(k) => {
                    g.input.key_event(k, true, false);
                    self.releases.push((0, Release::Key(k)));
                    return out;
                }
                Cmd::Hold(k, n) => {
                    g.input.key_event(k, true, false);
                    self.releases.push((n.saturating_sub(1), Release::Key(k)));
                }
                Cmd::Type(text) => {
                    for c in text.chars() {
                        g.input.char_event(c);
                    }
                    return out;
                }
                Cmd::Scroll(y) => {
                    g.input.scroll(y);
                    return out;
                }
                Cmd::Capture(name) => {
                    // L'image de cette frame-ci est celle d'après le tick : capturée à la fin.
                    out.capture = Some(name);
                    return out;
                }
                Cmd::ExpectState(s) => {
                    if g.state() != s {
                        let msg = format!("état {:?} attendu, {:?} obtenu", s, g.state());
                        self.fail(msg);
                    }
                }
                Cmd::WaitMeshes(n, max) => {
                    if meshes < n {
                        if max == 0 {
                            self.fail(format!("{n} chunks maillés attendus, {meshes} obtenus"));
                        } else {
                            self.pc -= 1;
                            self.cmds[self.pc] = Cmd::WaitMeshes(n, max - 1);
                            return out;
                        }
                    }
                }
                Cmd::SpawnMob(id, dist) => self.spawn_mob(g, &id, dist),
                Cmd::SpawnParticles(id, n) => self.spawn_particles(g, &id, n),
                Cmd::Give(name, n) => {
                    if let Some(item) = g.regs.items.id_of(&name) {
                        if let Some(inv) = g.session_mut().inventory_mut() {
                            inv.add_stack(nb_ecs::ItemStack::new(item, n));
                        }
                    } else {
                        eprintln!("give : objet inconnu {name}");
                    }
                }
                Cmd::Vitals(h, w, s) => {
                    if let Some(p) = g.session().player() {
                        if let Some(v) = g.session_mut().sim.entities.get_mut::<nb_ecs::Vitals>(p) {
                            v.hunger = h;
                            v.warmth = w;
                            v.stamina = s;
                        }
                    }
                }
                Cmd::ThirdPerson => g.toggle_third_person(),
                Cmd::Quit => {
                    out.quit = true;
                    self.pc = self.cmds.len();
                    return out;
                }
            }
        }
        out
    }

    fn ahead(g: &Game, dist: f32) -> Vec3 {
        let c = g.camera();
        let yaw = c.yaw_deg.to_radians();
        let pitch = c.pitch_deg.to_radians();
        // Caméra JOML : lacet 0 = regard vers −Z.
        let dir = Vec3::new(
            yaw.sin() * pitch.cos(),
            -pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        c.position + dir * dist
    }

    fn spawn_mob(&mut self, g: &mut Game, id: &str, dist: f32) {
        let Some(cfg) = g.regs.mobs.get(id).cloned() else {
            return self.fail(format!("mob inconnu : {id}"));
        };
        let p = Self::ahead(g, dist);
        let feet = f64::from(p.y) - 1.62;
        let e = &mut g.session_mut().sim.entities;
        nb_ecs::spawn::spawn_mob(e, &cfg, f64::from(p.x), feet, f64::from(p.z), &mut self.rng);
    }

    fn spawn_particles(&mut self, g: &mut Game, id: &str, n: u32) {
        if let Err(e) = g.fx_mut().spawn_named(id, n) {
            self.fail(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_and_keys() {
        let s = "# menu\nwait 3\nclick 640 380\nkey esc\nhold W 10\ntype My World\ncapture menu\nexpect-state Playing\nwait-meshes 4 600\nspawn-mob reindeer 6\nquit\n";
        let c = parse(s).unwrap();
        assert_eq!(c[0], Cmd::Wait(3));
        assert_eq!(c[1], Cmd::Click(0, Some((640.0, 380.0))));
        assert_eq!(parse("click").unwrap(), [Cmd::Click(0, None)]);
        assert_eq!(c[2], Cmd::Key(keys::KEY_ESCAPE));
        assert_eq!(c[3], Cmd::Hold(keys::KEY_W, 10));
        assert_eq!(c[4], Cmd::Type("My World".into()));
        assert_eq!(c[5], Cmd::Capture("menu".into()));
        assert_eq!(c[6], Cmd::ExpectState(GameState::Playing));
        assert_eq!(c[7], Cmd::WaitMeshes(4, 600));
        assert_eq!(c[8], Cmd::SpawnMob("reindeer".into(), 6.0));
        assert_eq!(c[9], Cmd::Quit);
        assert!(parse("bogus 1").is_err());
        assert_eq!(key_code("F3"), Some(292));
        assert_eq!(key_code("e"), Some(keys::KEY_E));
        assert_eq!(key_code("257"), Some(257));
    }
}
