//! Écran « Multiplayer » (réseau local) : parties trouvées automatiquement, saisie d'une adresse
//! pour les réseaux sans découverte, nom du joueur. Aucune logique réseau ici : l'écran renvoie
//! une [`MultiplayerAction`] que nb-game applique.
//!
//! Champs de saisie : `buffers.command` = adresse, `buffers.secondary` = nom du joueur.

use super::common::{draw_background, draw_title, MouseEdge, Scroll, CONTENT_W, SPACER_H};
use crate::draw::{gray, DrawList, Scissor, WHITE, YELLOW};
use crate::font;
use crate::input::{TypingBuffers, UiInput};
use crate::widgets::{button, hit_test};

const BOTTOM_H: i32 = 150;
const ROW_H: i32 = 64;
const ROW_GAP: i32 = 6;
const FIELD_H: i32 = 44;
const BTN_H: i32 = 50;
const GAP: i32 = 10;
const JOIN_W: i32 = 320;
const SMALL_W: i32 = (CONTENT_W - JOIN_W - 2 * GAP) / 2;
/// Délai maximal (secondes) entre deux clics pour un double-clic sur une partie.
const DOUBLE_CLICK: f64 = 0.45;
const RED: [f32; 4] = [1.0, 0.45, 0.4, 1.0];

/// Une partie affichée dans la liste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRow {
    /// Nom du monde.
    pub title: String,
    /// « Hosted by Léa · 2/8 players · 192.168.1.5:25570 ».
    pub subtitle: String,
    /// `adresse:port` à joindre.
    pub address: String,
    /// Partie pleine : non joignable.
    pub full: bool,
}

/// Données fournies par le jeu à chaque image.
#[derive(Clone, Copy, Debug)]
pub struct MultiplayerView<'a> {
    pub servers: &'a [ServerRow],
    /// Connexion en cours (adresse).
    pub connecting: Option<&'a str>,
    /// Message d'état : texte, est-ce une erreur.
    pub message: Option<(&'a str, bool)>,
    /// La recherche automatique est impossible (message d'explication).
    pub search_problem: Option<&'a str>,
}

/// Actions de l'écran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiplayerAction {
    Back,
    Refresh,
    Join { address: String, name: String },
    CancelJoin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Name,
    Address,
}

/// État de l'écran.
#[derive(Clone, Debug, PartialEq)]
pub struct MultiplayerScreen {
    selected: Option<usize>,
    focus: Focus,
    cursor_blink: f64,
    clock: f64,
    mouse: MouseEdge,
    scroll: Scroll,
    last_click: Option<(usize, f64)>,
    typing: bool,
}

impl Default for MultiplayerScreen {
    fn default() -> Self {
        Self {
            selected: None,
            focus: Focus::Address,
            cursor_blink: 0.0,
            clock: 0.0,
            mouse: MouseEdge::default(),
            scroll: Scroll::default(),
            last_click: None,
            typing: false,
        }
    }
}

impl MultiplayerScreen {
    pub fn new() -> Self {
        Self::default()
    }

    /// Entrée dans l'écran : champs préremplis, saisie ouverte sur l'adresse.
    pub fn enter(&mut self, b: &mut TypingBuffers, name: &str, last_address: &str) {
        *self = Self::default();
        b.command = last_address.to_owned();
        b.secondary = name.to_owned();
        b.typing_mode = true;
        b.suppress_chat_filter = true;
        b.use_secondary = false;
        self.typing = true;
    }

    pub fn exit(&mut self, b: &mut TypingBuffers) {
        if self.typing {
            self.typing = false;
            b.typing_mode = false;
            b.suppress_chat_filter = false;
            b.use_secondary = false;
            b.secondary.clear();
            b.command.clear();
        }
    }

    fn focus_on(&mut self, f: Focus, b: &mut TypingBuffers) {
        self.focus = f;
        b.use_secondary = f == Focus::Name;
    }

    // Géométrie ---------------------------------------------------------------------------

    fn cx(w: i32) -> i32 {
        (w - CONTENT_W) / 2
    }
    const NAME_Y: i32 = SPACER_H + 8;
    const LIST_TOP: i32 = SPACER_H + 64;

    fn name_rect(w: i32) -> (i32, i32, i32, i32) {
        (Self::cx(w) + 210, Self::NAME_Y, 340, FIELD_H)
    }
    fn addr_rect(w: i32, h: i32) -> (i32, i32, i32, i32) {
        (
            Self::cx(w) + 310,
            h - BOTTOM_H + 10,
            CONTENT_W - 310,
            FIELD_H,
        )
    }
    fn row_y(h: i32) -> i32 {
        h - BOTTOM_H + 10 + FIELD_H + 10
    }
    fn join_rect(w: i32, h: i32) -> (i32, i32, i32, i32) {
        (Self::cx(w), Self::row_y(h), JOIN_W, BTN_H)
    }
    fn refresh_rect(w: i32, h: i32) -> (i32, i32, i32, i32) {
        (Self::cx(w) + JOIN_W + GAP, Self::row_y(h), SMALL_W, BTN_H)
    }
    fn back_rect(w: i32, h: i32) -> (i32, i32, i32, i32) {
        (
            Self::cx(w) + JOIN_W + GAP + SMALL_W + GAP,
            Self::row_y(h),
            SMALL_W,
            BTN_H,
        )
    }
    fn row_rect(w: i32, i: usize, scroll: i32) -> (i32, i32, i32, i32) {
        (
            Self::cx(w),
            Self::LIST_TOP + 8 + i as i32 * (ROW_H + ROW_GAP) - scroll,
            CONTENT_W,
            ROW_H,
        )
    }

    // Logique -----------------------------------------------------------------------------

    pub fn update(
        &mut self,
        input: &UiInput,
        b: &mut TypingBuffers,
        view: &MultiplayerView<'_>,
    ) -> Option<MultiplayerAction> {
        self.cursor_blink = (self.cursor_blink + input.dt) % 1.0;
        self.clock += input.dt;
        let clicked = self.mouse.update(input);
        let (w, h) = (input.width, input.height);

        if view.connecting.is_some() {
            let (x, y, bw, bh) = Self::join_rect(w, h);
            if input.cancel_pressed
                || input.pause_pressed
                || (clicked && hit_test(input.mouse_x, input.mouse_y, x, y, bw, bh))
            {
                return Some(MultiplayerAction::CancelJoin);
            }
            return None;
        }
        if input.cancel_pressed || input.pause_pressed {
            return Some(MultiplayerAction::Back);
        }
        if let Some(i) = self.selected {
            if i >= view.servers.len() {
                self.selected = None;
            }
        }
        let list_h = h - Self::LIST_TOP - BOTTOM_H;
        let content = 8 + view.servers.len() as i32 * (ROW_H + ROW_GAP);
        self.scroll
            .apply(input.scroll_y, ROW_H + ROW_GAP, content, list_h);

        if input.enter_pressed {
            return self.join(b);
        }
        if !clicked {
            return None;
        }
        let (mx, my) = (input.mouse_x, input.mouse_y);
        let (x, y, bw, bh) = Self::name_rect(w);
        if hit_test(mx, my, x, y, bw, bh) {
            self.focus_on(Focus::Name, b);
            return None;
        }
        let (x, y, bw, bh) = Self::addr_rect(w, h);
        if hit_test(mx, my, x, y, bw, bh) {
            self.focus_on(Focus::Address, b);
            return None;
        }
        let (x, y, bw, bh) = Self::join_rect(w, h);
        if hit_test(mx, my, x, y, bw, bh) {
            return self.join(b);
        }
        let (x, y, bw, bh) = Self::refresh_rect(w, h);
        if hit_test(mx, my, x, y, bw, bh) {
            return Some(MultiplayerAction::Refresh);
        }
        let (x, y, bw, bh) = Self::back_rect(w, h);
        if hit_test(mx, my, x, y, bw, bh) {
            return Some(MultiplayerAction::Back);
        }
        if my >= f64::from(Self::LIST_TOP) && my < f64::from(h - BOTTOM_H) {
            for (i, s) in view.servers.iter().enumerate() {
                let (x, y, bw, bh) = Self::row_rect(w, i, self.scroll.offset);
                if hit_test(mx, my, x, y, bw, bh) {
                    self.selected = Some(i);
                    if !s.full {
                        b.command = s.address.clone();
                    }
                    let double = self
                        .last_click
                        .is_some_and(|(j, t)| j == i && self.clock - t < DOUBLE_CLICK);
                    self.last_click = Some((i, self.clock));
                    if double && !s.full {
                        return self.join(b);
                    }
                    return None;
                }
            }
        }
        None
    }

    fn join(&mut self, b: &TypingBuffers) -> Option<MultiplayerAction> {
        Some(MultiplayerAction::Join {
            address: b.command.trim().to_owned(),
            name: b.secondary.trim().to_owned(),
        })
    }

    // Dessin ------------------------------------------------------------------------------

    fn field(dl: &mut DrawList, r: (i32, i32, i32, i32), active: bool, text: &str, hint: &str) {
        let (x, y, w, h) = r;
        dl.fill(
            x as f32,
            y as f32,
            w as f32,
            h as f32,
            gray(if active { 0.30 } else { 0.20 }),
        );
        if text.is_empty() && !active {
            dl.text(
                hint,
                (x + 8) as f32,
                (y + 13) as f32,
                2.0,
                [0.6, 0.6, 0.6, 1.0],
            );
        } else {
            dl.text(text, (x + 8) as f32, (y + 13) as f32, 2.0, WHITE);
        }
    }

    pub fn draw(
        &self,
        dl: &mut DrawList,
        input: &UiInput,
        b: &TypingBuffers,
        view: &MultiplayerView<'_>,
    ) {
        let (w, h) = (input.width, input.height);
        let cx = Self::cx(w);
        draw_background(dl, w, h, BOTTOM_H);
        draw_title(dl, "Multiplayer (LAN)", w);
        let blink = |on: bool| {
            if on && self.cursor_blink < 0.5 {
                "|"
            } else {
                ""
            }
        };
        let connecting = view.connecting.is_some();

        dl.text(
            "Your name:",
            cx as f32,
            (Self::NAME_Y + 13) as f32,
            2.0,
            WHITE,
        );
        let name_on = self.focus == Focus::Name && !connecting;
        Self::field(
            dl,
            Self::name_rect(w),
            name_on,
            &format!("{}{}", b.secondary, blink(name_on)),
            "Player",
        );
        dl.text(
            "Games on your network",
            cx as f32,
            (Self::LIST_TOP - 22) as f32,
            2.0,
            WHITE,
        );

        // Liste.
        let list_bottom = h - BOTTOM_H;
        dl.set_scissor(Some(Scissor::from_gl(
            0,
            BOTTOM_H,
            w,
            list_bottom - Self::LIST_TOP,
            h,
        )));
        if view.servers.is_empty() {
            let lines: [(&str, f32, [f32; 4]); 3] = if let Some(p) = view.search_problem {
                [
                    (p, 1.8, RED),
                    ("Type the host's address below instead.", 2.0, WHITE),
                    ("", 2.0, WHITE),
                ]
            } else {
                [
                    ("Searching for games on your network...", 2.5, WHITE),
                    (
                        "The host opens the game with ESC > OPEN TO LAN.",
                        1.8,
                        gray(0.8),
                    ),
                    (
                        "Nothing shows up? Type the host's address below.",
                        1.8,
                        gray(0.8),
                    ),
                ]
            };
            let mut y = Self::LIST_TOP as f32 + 40.0;
            for (t, s, c) in lines {
                let tw = font::measure_width(t, s);
                dl.text(t, w as f32 / 2.0 - tw / 2.0, y, s, c);
                y += 9.0 * s + 14.0;
            }
        } else {
            for (i, s) in view.servers.iter().enumerate() {
                let (x, y, bw, bh) = Self::row_rect(w, i, self.scroll.offset);
                if y + bh < Self::LIST_TOP || y > list_bottom {
                    continue;
                }
                let g = if self.selected == Some(i) { 0.30 } else { 0.15 };
                dl.fill(x as f32, y as f32, bw as f32, bh as f32, [g, g, g, 0.9]);
                let col = if s.full { gray(0.6) } else { WHITE };
                dl.text(&s.title, (x + 12) as f32, (y + 10) as f32, 2.4, col);
                dl.text(
                    &s.subtitle,
                    (x + 12) as f32,
                    (y + 40) as f32,
                    1.8,
                    gray(0.8),
                );
            }
        }
        dl.set_scissor(None);

        // Bas de l'écran.
        let (ax, ay, aw, _) = Self::addr_rect(w, h);
        dl.text(
            "Or type an address:",
            cx as f32,
            (ay + 13) as f32,
            2.0,
            WHITE,
        );
        let addr_on = self.focus == Focus::Address && !connecting;
        Self::field(
            dl,
            (ax, ay, aw, FIELD_H),
            addr_on,
            &format!("{}{}", b.command, blink(addr_on)),
            "192.168.1.20:25570",
        );
        let (jx, jy, jw, jh) = Self::join_rect(w, h);
        if let Some(t) = view.connecting {
            button(dl, jx, jy, jw, jh, "Cancel", true);
            let msg = format!("Connecting to {t}...");
            dl.text(
                &msg,
                (jx + jw + GAP + 8) as f32,
                (jy + 17) as f32,
                2.0,
                YELLOW,
            );
        } else {
            let (rx, ry, rw, rh) = Self::refresh_rect(w, h);
            let (bx, by, bw, bh) = Self::back_rect(w, h);
            button(
                dl,
                jx,
                jy,
                jw,
                jh,
                "Join Game",
                !b.command.trim().is_empty(),
            );
            button(dl, rx, ry, rw, rh, "Refresh", true);
            button(dl, bx, by, bw, bh, "Back", true);
        }
        if let Some((text, error)) = view.message {
            dl.text(
                text,
                cx as f32,
                (Self::row_y(h) + BTN_H + 12) as f32,
                1.9,
                if error { RED } else { YELLOW },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<ServerRow> {
        vec![
            ServerRow {
                title: "Mon monde".into(),
                subtitle: "Hosted by Lea".into(),
                address: "192.168.1.5:25570".into(),
                full: false,
            },
            ServerRow {
                title: "Plein".into(),
                subtitle: "8/8".into(),
                address: "192.168.1.6:25570".into(),
                full: true,
            },
        ]
    }

    fn view<'a>(s: &'a [ServerRow], connecting: Option<&'a str>) -> MultiplayerView<'a> {
        MultiplayerView {
            servers: s,
            connecting,
            message: None,
            search_problem: None,
        }
    }

    fn input() -> UiInput {
        UiInput {
            width: 1280,
            height: 720,
            dt: 0.016,
            ..Default::default()
        }
    }

    /// Un clic complet (enfoncé puis relâché) à `(x, y)`.
    fn click(
        sc: &mut MultiplayerScreen,
        b: &mut TypingBuffers,
        v: &MultiplayerView<'_>,
        x: f64,
        y: f64,
    ) -> Option<MultiplayerAction> {
        let mut i = input();
        i.mouse_x = x;
        i.mouse_y = y;
        let r = sc.update(
            &UiInput {
                mouse_down: true,
                ..i
            },
            b,
            v,
        );
        sc.update(&i, b, v);
        r
    }

    #[test]
    fn enter_opens_typing_with_prefilled_fields() {
        let mut sc = MultiplayerScreen::new();
        let mut b = TypingBuffers::default();
        sc.enter(&mut b, "Gabriel", "10.0.0.2:25570");
        assert!(b.typing_mode && b.suppress_chat_filter && !b.use_secondary);
        assert_eq!(
            (b.command.as_str(), b.secondary.as_str()),
            ("10.0.0.2:25570", "Gabriel")
        );
        sc.exit(&mut b);
        assert!(!b.typing_mode && b.command.is_empty() && b.secondary.is_empty());
    }

    #[test]
    fn clicking_a_game_fills_the_address_and_double_click_joins() {
        let servers = rows();
        let v = view(&servers, None);
        let mut sc = MultiplayerScreen::new();
        let mut b = TypingBuffers::default();
        sc.enter(&mut b, "Gab", "");
        // Première ligne : x = 240.., y = LIST_TOP + 8.
        let (x, y) = (640.0, f64::from(MultiplayerScreen::LIST_TOP + 8 + 20));
        assert_eq!(click(&mut sc, &mut b, &v, x, y), None);
        assert_eq!(b.command, "192.168.1.5:25570");
        assert_eq!(
            click(&mut sc, &mut b, &v, x, y),
            Some(MultiplayerAction::Join {
                address: "192.168.1.5:25570".into(),
                name: "Gab".into()
            })
        );
        // Partie pleine : l'adresse n'est pas reprise, pas de double-clic.
        let y2 = y + f64::from(ROW_H + ROW_GAP);
        b.command.clear();
        click(&mut sc, &mut b, &v, x, y2);
        assert_eq!(click(&mut sc, &mut b, &v, x, y2), None);
        assert!(b.command.is_empty());
    }

    #[test]
    fn buttons_focus_and_keys() {
        let servers = rows();
        let v = view(&servers, None);
        let mut sc = MultiplayerScreen::new();
        let mut b = TypingBuffers::default();
        sc.enter(&mut b, "Gab", "1.2.3.4");
        let (nx, ny, _, _) = MultiplayerScreen::name_rect(1280);
        click(&mut sc, &mut b, &v, f64::from(nx + 5), f64::from(ny + 5));
        assert!(b.use_secondary, "le champ du nom prend le focus");
        let (ax, ay, _, _) = MultiplayerScreen::addr_rect(1280, 720);
        click(&mut sc, &mut b, &v, f64::from(ax + 5), f64::from(ay + 5));
        assert!(!b.use_secondary);

        let (rx, ry, _, _) = MultiplayerScreen::refresh_rect(1280, 720);
        assert_eq!(
            click(&mut sc, &mut b, &v, f64::from(rx + 5), f64::from(ry + 5)),
            Some(MultiplayerAction::Refresh)
        );
        let (bx, by, _, _) = MultiplayerScreen::back_rect(1280, 720);
        assert_eq!(
            click(&mut sc, &mut b, &v, f64::from(bx + 5), f64::from(by + 5)),
            Some(MultiplayerAction::Back)
        );
        let (jx, jy, _, _) = MultiplayerScreen::join_rect(1280, 720);
        assert!(matches!(
            click(&mut sc, &mut b, &v, f64::from(jx + 5), f64::from(jy + 5)),
            Some(MultiplayerAction::Join { .. })
        ));
        // Entrée valide, Échap retourne.
        let r = sc.update(
            &UiInput {
                enter_pressed: true,
                ..input()
            },
            &mut b,
            &v,
        );
        assert!(matches!(r, Some(MultiplayerAction::Join { .. })));
        let r = sc.update(
            &UiInput {
                cancel_pressed: true,
                ..input()
            },
            &mut b,
            &v,
        );
        assert_eq!(r, Some(MultiplayerAction::Back));
    }

    #[test]
    fn while_connecting_only_cancel_works() {
        let servers = rows();
        let v = view(&servers, Some("1.2.3.4:25570"));
        let mut sc = MultiplayerScreen::new();
        let mut b = TypingBuffers::default();
        sc.enter(&mut b, "Gab", "");
        let (jx, jy, _, _) = MultiplayerScreen::join_rect(1280, 720);
        assert_eq!(
            click(&mut sc, &mut b, &v, f64::from(jx + 5), f64::from(jy + 5)),
            Some(MultiplayerAction::CancelJoin)
        );
        let r = sc.update(
            &UiInput {
                cancel_pressed: true,
                ..input()
            },
            &mut b,
            &v,
        );
        assert_eq!(r, Some(MultiplayerAction::CancelJoin));
        let r = sc.update(
            &UiInput {
                enter_pressed: true,
                ..input()
            },
            &mut b,
            &v,
        );
        assert_eq!(r, None, "pas de seconde connexion pendant la première");
    }

    #[test]
    fn draw_shows_games_hints_and_messages() {
        let servers = rows();
        let mut sc = MultiplayerScreen::new();
        let mut b = TypingBuffers::default();
        sc.enter(&mut b, "Gab", "");
        let mut dl = DrawList::new(1280, 720);
        sc.draw(&mut dl, &input(), &b, &view(&servers, None));
        assert!(!dl.vertices().is_empty());
        // Sans partie : texte d'aide ; avec un problème de recherche : l'explication.
        let mut dl = DrawList::new(1280, 720);
        let empty = MultiplayerView {
            servers: &[],
            connecting: None,
            message: Some(("Connection refused", true)),
            search_problem: Some("Network search unavailable"),
        };
        sc.draw(&mut dl, &input(), &b, &empty);
        assert!(!dl.vertices().is_empty());
    }
}
