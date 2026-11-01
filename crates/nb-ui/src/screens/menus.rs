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
}

const PLAY: u32 = 0;
const SETTINGS: u32 = 1;
const QUIT: u32 = 2;
const RESUME: u32 = 3;
const MAIN_MENU: u32 = 4;

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
            &[("PLAY", PLAY), ("SETTINGS", SETTINGS), ("QUIT", QUIT)],
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
}

impl PauseMenu {
    pub fn build(w: i32, h: i32) -> Panel {
        let mut ui = Panel::new(0, 0, w, h, None);
        ui.add_panel(buttons(
            h / 4,
            w,
            h * 3 / 4,
            &[
                ("RESUME", RESUME),
                ("SETTINGS", SETTINGS),
                ("MAIN MENU", MAIN_MENU),
            ],
        ));
        ui
    }

    pub fn enter(&mut self) {
        self.built = Built::default();
    }

    pub fn update(&mut self, input: &UiInput, ctx: &mut UiContext) -> Option<MenuAction> {
        let ui = self.built.ensure(input.width, input.height, || {
            Self::build(input.width, input.height)
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
        // Boutons 400×50 en x = 440, y = 360 / 420 / 480.
        assert_eq!(
            m.update(&click(640.0, 380.0), &mut ctx),
            Some(MenuAction::SwitchTo(GameState::WorldSelect))
        );
        assert_eq!(
            m.update(&click(441.0, 470.0), &mut ctx),
            Some(MenuAction::SwitchTo(GameState::Settings))
        );
        assert_eq!(ctx.previous_state, GameState::MainMenu);
        assert_eq!(
            m.update(&click(839.0, 529.0), &mut ctx),
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
