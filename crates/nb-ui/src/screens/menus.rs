//! Menu principal (`MainMenuState`) et menu pause (`PausedState`).

use crate::draw::DrawList;
use crate::input::UiInput;
use crate::state::{GameState, UiContext};
use crate::textures::UiTexture;
use crate::widgets::{Element, Layout, Panel};

/// Action d'un menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    SwitchTo(GameState),
    /// QUIT : `glfwSetWindowShouldClose`.
    Quit,
    /// MAIN MENU : sauvegarde du monde courant (s'il y en a un) puis MAIN_MENU.
    SaveAndMainMenu,
    /// OPEN TO LAN : le monde courant devient une partie multijoueur.
    OpenLan,
    /// CLOSE LAN : retour au solo (les joueurs connectés sont renvoyés).
    CloseLan,
}

/// Ce que le menu pause propose pour le réseau local.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LanPause {
    /// Pas de réseau possible (monde du menu).
    #[default]
    Hidden,
    /// Partie solo : « OPEN TO LAN » est proposé.
    CanOpen,
    /// On héberge : adresse à donner et joueurs présents.
    Hosting {
        address: String,
        players: Vec<String>,
    },
    /// On a rejoint la partie de `host`.
    Joined { host: String, ping_ms: Option<u32> },
}

const PLAY: u32 = 0;
const SETTINGS: u32 = 1;
const QUIT: u32 = 2;
const RESUME: u32 = 3;
const MAIN_MENU: u32 = 4;
const MULTIPLAYER: u32 = 5;
const OPEN_LAN: u32 = 6;
const CLOSE_LAN: u32 = 7;

/// Panneau reconstruit quand la fenêtre change de taille (`buildUI`).
#[derive(Clone, Debug, Default, PartialEq)]
struct Built {
    ui: Option<Panel>,
    size: (i32, i32),
}

impl Built {
    fn ensure(&mut self, w: i32, h: i32, build: impl FnOnce() -> Panel) -> &Panel {
        if self.ui.is_none() || self.size != (w, h) {
            self.size = (w, h);
            self.ui = Some(build());
        }
        self.ui.as_ref().expect("panneau construit")
    }
}

fn buttons(y: i32, w: i32, h: i32, entries: &[(&str, u32)]) -> Panel {
    let mut p = Panel::new(0, y, w, h, Some(Layout::Vertical { spacing: 10 }));
    for &(label, id) in entries {
        p.add(Element::button(400, 50, label, id));
    }
    p
}

/// Écran titre.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MainMenu {
    built: Built,
}

impl MainMenu {
    pub fn build(w: i32, h: i32) -> Panel {
        let mut ui = Panel::new(0, 0, w, h, None);
        ui.add(Element::image(
            (w - 800) / 2,
            h / 4 - 100,
            800,
            200,
            UiTexture::Logo,
        ));
        ui.add_panel(buttons(
            h / 2,
            w,
            h / 2,
            &[
                ("PLAY", PLAY),
                ("MULTIPLAYER", MULTIPLAYER),
                ("SETTINGS", SETTINGS),
                ("QUIT", QUIT),
            ],
        ));
        ui
    }

    /// `enter()` : panneau à reconstruire.
    pub fn enter(&mut self) {
        self.built = Built::default();
    }

    /// `update` (la rotation de la caméra de fond est faite par nb-game).
    pub fn update(&mut self, input: &UiInput, ctx: &mut UiContext) -> Option<MenuAction> {
        let ui = self.built.ensure(input.width, input.height, || {
            Self::build(input.width, input.height)
        });
        if !input.primary_pressed {
            return None;
        }
        let mut out = None;
        for id in ui.handle_click(input.mouse_x, input.mouse_y) {
            out = Some(match id {
                PLAY => MenuAction::SwitchTo(GameState::WorldSelect),
                MULTIPLAYER => MenuAction::SwitchTo(GameState::Multiplayer),
                SETTINGS => {
                    ctx.previous_state = GameState::MainMenu;
                    MenuAction::SwitchTo(GameState::Settings)
                }
                _ => MenuAction::Quit,
            });
        }
        out
    }

    pub fn draw(&self, dl: &mut DrawList, input: &UiInput) {
        if let Some(ui) = &self.built.ui {
            ui.draw(dl, input.mouse_x, input.mouse_y);
        }
    }
}

/// Menu pause (voile noir α 0,6 sur la scène).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PauseMenu {
    built: Built,
    lan: LanPause,
}

impl PauseMenu {
    pub fn build(w: i32, h: i32) -> Panel {
        Self::build_with(w, h, &LanPause::Hidden)
    }

    fn build_with(w: i32, h: i32, lan: &LanPause) -> Panel {
        let mut ui = Panel::new(0, 0, w, h, None);
        let mut entries: Vec<(&str, u32)> = vec![("RESUME", RESUME), ("SETTINGS", SETTINGS)];
        match lan {
            LanPause::CanOpen => entries.push(("OPEN TO LAN", OPEN_LAN)),
            LanPause::Hosting { .. } => entries.push(("CLOSE LAN", CLOSE_LAN)),
            LanPause::Hidden | LanPause::Joined { .. } => {}
        }
        entries.push((
            if matches!(lan, LanPause::Joined { .. }) {
                "LEAVE GAME"
            } else {
                "MAIN MENU"
            },
            MAIN_MENU,
        ));
        ui.add_panel(buttons(h / 4, w, h * 3 / 4, &entries));
        ui
    }

    pub fn enter(&mut self) {
        self.built = Built::default();
    }

    /// Réseau local proposé par le menu (reconstruit les boutons si cela change).
    pub fn set_lan(&mut self, lan: LanPause) {
        let layout_changed = std::mem::discriminant(&self.lan) != std::mem::discriminant(&lan);
        self.lan = lan;
        if layout_changed {
            self.built = Built::default();
        }
    }

    pub fn update(&mut self, input: &UiInput, ctx: &mut UiContext) -> Option<MenuAction> {
        let lan = self.lan.clone();
        let ui = self.built.ensure(input.width, input.height, || {
            Self::build_with(input.width, input.height, &lan)
        });
        if input.pause_pressed {
            return Some(MenuAction::SwitchTo(GameState::Playing));
        }
        if !input.primary_pressed {
            return None;
        }
        let mut out = None;
        for id in ui.handle_click(input.mouse_x, input.mouse_y) {
            out = Some(match id {
                RESUME => MenuAction::SwitchTo(GameState::Playing),
                SETTINGS => {
                    ctx.previous_state = GameState::Paused;
                    MenuAction::SwitchTo(GameState::Settings)
                }
                OPEN_LAN => MenuAction::OpenLan,
                CLOSE_LAN => MenuAction::CloseLan,
                _ => MenuAction::SaveAndMainMenu,
            });
        }
        out
    }

    pub fn draw(&self, dl: &mut DrawList, input: &UiInput) {
        dl.fill(
            0.0,
            0.0,
            input.width as f32,
            input.height as f32,
            [0.0, 0.0, 0.0, 0.6],
        );
        if let Some(ui) = &self.built.ui {
            ui.draw(dl, input.mouse_x, input.mouse_y);
        }
        // Informations sur la partie en réseau, sous les boutons.
        let buttons_n = 3 + usize::from(!matches!(
            self.lan,
            LanPause::Hidden | LanPause::Joined { .. }
        ));
        let mut y = (input.height / 4 + buttons_n as i32 * 60 + 14) as f32;
        let line = |dl: &mut DrawList, y: &mut f32, t: &str, s: f32, c: [f32; 4]| {
            let tw = crate::font::measure_width(t, s);
            dl.text(t, input.width as f32 / 2.0 - tw / 2.0, *y, s, c);
            *y += 9.0 * s + 8.0;
        };
        match &self.lan {
            LanPause::Hosting { address, players } => {
                line(
                    dl,
                    &mut y,
                    "LAN game open - friends join with:",
                    1.8,
                    crate::draw::gray(0.85),
                );
                line(dl, &mut y, address, 2.6, crate::draw::YELLOW);
                let names = if players.len() <= 1 {
                    "Waiting for players... (Multiplayer > Join Game)".to_owned()
                } else {
                    format!("Players: {}", players.join(", "))
                };
                line(dl, &mut y, &names, 1.8, crate::draw::gray(0.85));
            }
            LanPause::Joined { host, ping_ms } => {
                line(
                    dl,
                    &mut y,
                    &format!("Playing in {host}'s game"),
                    2.0,
                    crate::draw::YELLOW,
                );
                if let Some(p) = ping_ms {
                    line(
                        dl,
                        &mut y,
                        &format!("Latency: {p} ms"),
                        1.8,
                        crate::draw::gray(0.85),
                    );
                }
            }
            LanPause::CanOpen | LanPause::Hidden => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::TextureRef;

    fn click(x: f64, y: f64) -> UiInput {
        UiInput {
            width: 1280,
            height: 720,
            mouse_x: x,
            mouse_y: y,
            primary_pressed: true,
            ..Default::default()
        }
    }

    #[test]
    fn main_menu_buttons() {
        let mut m = MainMenu::default();
        let mut ctx = UiContext {
            previous_state: GameState::Paused,
            ..Default::default()
        };
        // Boutons 400×50 en x = 440, y = 360 / 420 / 480 / 540.
        assert_eq!(
            m.update(&click(640.0, 380.0), &mut ctx),
            Some(MenuAction::SwitchTo(GameState::WorldSelect))
        );
        assert_eq!(
            m.update(&click(640.0, 440.0), &mut ctx),
            Some(MenuAction::SwitchTo(GameState::Multiplayer))
        );
        assert_eq!(
            m.update(&click(441.0, 520.0), &mut ctx),
            Some(MenuAction::SwitchTo(GameState::Settings))
        );
        assert_eq!(ctx.previous_state, GameState::MainMenu);
        assert_eq!(
            m.update(&click(839.0, 589.0), &mut ctx),
            Some(MenuAction::Quit)
        );
        assert_eq!(
            m.update(&click(640.0, 415.0), &mut ctx),
            None,
            "entre deux boutons"
        );
        assert_eq!(
            m.update(&click(640.0, 100.0), &mut ctx),
            None,
            "sur le logo"
        );
        let mut dl = DrawList::new(1280, 720);
        m.draw(&mut dl, &click(0.0, 0.0));
        assert_eq!(dl.batches()[0].texture, TextureRef::Ui(UiTexture::Logo));
        assert_eq!(dl.vertices()[0].pos, [240.0, 80.0]);
    }

    #[test]
    fn pause_menu_lan_buttons() {
        let mut m = PauseMenu::default();
        let mut ctx = UiContext::default();
        // Solo : « OPEN TO LAN » s'insère avant « MAIN MENU » (y = 180, 240, 300, 360).
        m.set_lan(LanPause::CanOpen);
        assert_eq!(
            m.update(&click(640.0, 320.0), &mut ctx),
            Some(MenuAction::OpenLan)
        );
        assert_eq!(
            m.update(&click(640.0, 380.0), &mut ctx),
            Some(MenuAction::SaveAndMainMenu)
        );
        // Hôte : « CLOSE LAN » au même endroit, avec l'adresse affichée.
        m.set_lan(LanPause::Hosting {
            address: "192.168.1.5:25570".into(),
            players: vec!["Gab".into(), "Lea".into()],
        });
        assert_eq!(
            m.update(&click(640.0, 320.0), &mut ctx),
            Some(MenuAction::CloseLan)
        );
        let mut dl = DrawList::new(1280, 720);
        m.draw(&mut dl, &click(0.0, 0.0));
        assert!(dl.vertices().len() > 100, "boutons + adresse + joueurs");
        // Client : pas d'ouverture au LAN, « LEAVE GAME » remplace « MAIN MENU » en 3e position.
        m.set_lan(LanPause::Joined {
            host: "Gab".into(),
            ping_ms: Some(3),
        });
        assert_eq!(
            m.update(&click(640.0, 320.0), &mut ctx),
            Some(MenuAction::SaveAndMainMenu)
        );
        assert_eq!(m.update(&click(640.0, 380.0), &mut ctx), None);
    }

    #[test]
    fn pause_menu() {
        let mut m = PauseMenu::default();
        let mut ctx = UiContext::default();
        let mut i = click(640.0, 200.0);
        assert_eq!(
            m.update(&i, &mut ctx),
            Some(MenuAction::SwitchTo(GameState::Playing))
        );
        i.mouse_y = 260.0;
        assert_eq!(
            m.update(&i, &mut ctx),
            Some(MenuAction::SwitchTo(GameState::Settings))
        );
        assert_eq!(ctx.previous_state, GameState::Paused);
        i.mouse_y = 320.0;
        assert_eq!(m.update(&i, &mut ctx), Some(MenuAction::SaveAndMainMenu));
        i.primary_pressed = false;
        i.pause_pressed = true;
        assert_eq!(
            m.update(&i, &mut ctx),
            Some(MenuAction::SwitchTo(GameState::Playing))
        );
        let mut dl = DrawList::new(1280, 720);
        m.draw(&mut dl, &i);
        assert_eq!(dl.vertices()[0].color, [0.0, 0.0, 0.0, 0.6]);
    }
}
