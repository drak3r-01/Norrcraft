//! Éléments d'interface (`UIElement` et sous-classes), layouts et helpers de dessin
//! partagés par les écrans (`renderButton`, `renderTabButton`, `renderLabel`…).

use crate::draw::{gray, DrawList, Rgba, WHITE};
use crate::font;
use crate::textures::UiTexture;

/// `hitTest` / `isHovered` Java : bornes **incluses** des deux côtés.
pub fn hit_test(mx: f64, my: f64, x: i32, y: i32, w: i32, h: i32) -> bool {
    mx >= x as f64 && mx <= (x + w) as f64 && my >= y as f64 && my <= (y + h) as f64
}

/// Identifiant d'action renvoyé par `onClick` (les `Runnable` Java).
pub type ActionId = u32;

/// Nature d'un élément.
#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    /// `UIImage`.
    Image(UiTexture),
    /// `UITextureRegion` (UV Java `u, v, uw, vh`).
    Region { tex: UiTexture, gl_uv: [f32; 4] },
    /// `UITextButton` (texture bouton, libellé échelle 2 centré).
    TextButton {
        tex: UiTexture,
        label: String,
        action: Option<ActionId>,
    },
    /// `UISlot` : surbrillance blanche α 0,4 au survol.
    Slot {
        highlight: Option<UiTexture>,
        action: Option<ActionId>,
    },
    /// `UIPanel`.
    Panel(Panel),
}

/// `UIElement(x, y, width, height)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub kind: ElementKind,
}

impl Element {
    pub fn new(x: i32, y: i32, w: i32, h: i32, kind: ElementKind) -> Self {
        Self { x, y, w, h, kind }
    }

    pub fn image(x: i32, y: i32, w: i32, h: i32, tex: UiTexture) -> Self {
        Self::new(x, y, w, h, ElementKind::Image(tex))
    }

    pub fn button(w: i32, h: i32, label: &str, action: ActionId) -> Self {
        Self::new(
            0,
            0,
            w,
            h,
            ElementKind::TextButton {
                tex: UiTexture::Button,
                label: label.to_owned(),
                action: Some(action),
            },
        )
    }

    pub fn is_hovered(&self, mx: f64, my: f64) -> bool {
        hit_test(mx, my, self.x, self.y, self.w, self.h)
    }

    /// `setPosition` ; un panneau garde ses enfants où ils sont (comme en Java).
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
        if let ElementKind::Panel(p) = &mut self.kind {
            p.x = x;
            p.y = y;
        }
    }

    pub fn draw(&self, dl: &mut DrawList, mx: f64, my: f64) {
        let (x, y, w, h) = (self.x as f32, self.y as f32, self.w as f32, self.h as f32);
        match &self.kind {
            ElementKind::Image(t) => dl.image(*t, x, y, w, h, WHITE),
            ElementKind::Region { tex, gl_uv } => dl.region_gl(*tex, x, y, w, h, *gl_uv, WHITE),
            ElementKind::TextButton { tex, label, .. } => {
                dl.image(*tex, x, y, w, h, WHITE);
                centered_label(dl, label, x, y, w, h, 2.0);
            }
            ElementKind::Slot { highlight, .. } => {
                if let Some(t) = highlight {
                    if self.is_hovered(mx, my) {
                        dl.image(*t, x, y, w, h, [1.0, 1.0, 1.0, 0.4]);
                    }
                }
            }
            ElementKind::Panel(p) => p.draw(dl, mx, my),
        }
    }

    /// `onClick` : action de l'élément (les panneaux n'en ont pas).
    pub fn action(&self) -> Option<ActionId> {
        match &self.kind {
            ElementKind::TextButton { action, .. } | ElementKind::Slot { action, .. } => *action,
            _ => None,
        }
    }
}

/// `UILayout`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// `VerticalLayout(spacing)` : centré horizontalement, empilé.
    Vertical { spacing: i32 },
    /// `UIGridLayout(columns, spacingX, spacingY)`.
    Grid {
        columns: i32,
        spacing_x: i32,
        spacing_y: i32,
    },
}

impl Layout {
    /// `applyLayout(elements, parentX, parentY, parentWidth, parentHeight)`.
    pub fn apply(&self, elements: &mut [Element], px: i32, py: i32, pw: i32, _ph: i32) {
        match *self {
            Layout::Vertical { spacing } => {
                let mut cy = py;
                for e in elements {
                    let cx = px + (pw - e.w) / 2;
                    e.set_position(cx, cy);
                    cy += e.h + spacing;
                }
            }
            Layout::Grid {
                columns,
                spacing_x,
                spacing_y,
            } => {
                let (mut row, mut col) = (0, 0);
                for e in elements {
                    let ex = px + col * (e.w + spacing_x);
                    let ey = py + row * (e.h + spacing_y);
                    e.set_position(ex, ey);
                    col += 1;
                    if col >= columns {
                        col = 0;
                        row += 1;
                    }
                }
            }
        }
    }
}

/// `UIPanel` : conteneur, layout réappliqué à chaque ajout.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub layout: Option<Layout>,
    pub children: Vec<Element>,
}

impl Panel {
    pub fn new(x: i32, y: i32, w: i32, h: i32, layout: Option<Layout>) -> Self {
        Self {
            x,
            y,
            w,
            h,
            layout,
            children: Vec::new(),
        }
    }

    pub fn add(&mut self, e: Element) {
        self.children.push(e);
        if let Some(l) = self.layout {
            l.apply(&mut self.children, self.x, self.y, self.w, self.h);
        }
    }

    /// Ajoute un sous-panneau.
    pub fn add_panel(&mut self, p: Panel) {
        let (x, y, w, h) = (p.x, p.y, p.w, p.h);
        self.add(Element::new(x, y, w, h, ElementKind::Panel(p)));
    }

    pub fn draw(&self, dl: &mut DrawList, mx: f64, my: f64) {
        for c in &self.children {
            c.draw(dl, mx, my);
        }
    }

    /// `handleMouseClick` : actions de **tous** les enfants survolés, dans l'ordre, en
    /// descendant dans les sous-panneaux survolés.
    pub fn handle_click(&self, mx: f64, my: f64) -> Vec<ActionId> {
        let mut out = Vec::new();
        self.collect_clicks(mx, my, &mut out);
        out
    }

    fn collect_clicks(&self, mx: f64, my: f64, out: &mut Vec<ActionId>) {
        for c in &self.children {
            if c.is_hovered(mx, my) {
                if let Some(a) = c.action() {
                    out.push(a);
                }
                if let ElementKind::Panel(p) = &c.kind {
                    p.collect_clicks(mx, my, out);
                }
            }
        }
    }
}

/// `UISlider` (piste 8 px, poignée 14 px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slider {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub min: f32,
    pub max: f32,
    pub value: f32,
}

impl Slider {
    const TRACK_H: i32 = 8;
    const THUMB_W: i32 = 14;

    pub fn new(w: i32, h: i32, min: f32, max: f32, value: f32) -> Self {
        Self {
            x: 0,
            y: 0,
            w,
            h,
            min,
            max,
            value: value.clamp(min, max),
        }
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn contains(&self, mx: f64, my: f64) -> bool {
        hit_test(mx, my, self.x, self.y, self.w, self.h)
    }

    /// `setValueFromMouseX` : `t = (mx - x) / width` borné à [0, 1].
    pub fn set_value_from_mouse_x(&mut self, mx: f64) {
        let t = ((mx - self.x as f64) / self.w as f64) as f32;
        let t = t.clamp(0.0, 1.0);
        self.value = self.min + t * (self.max - self.min);
    }

    pub fn draw(&self, dl: &mut DrawList) {
        let track_y = self.y + (self.h - Self::TRACK_H) / 2;
        dl.fill(
            self.x as f32,
            track_y as f32,
            self.w as f32,
            Self::TRACK_H as f32,
            gray(0.35),
        );
        let t = (self.value - self.min) / (self.max - self.min);
        let fill = ((t * self.w as f32) as i32).max(0);
        if fill > 0 {
            dl.fill(
                self.x as f32,
                track_y as f32,
                fill as f32,
                Self::TRACK_H as f32,
                gray(0.85),
            );
        }
        let thumb_x = self.x + (t * self.w as f32) as i32 - Self::THUMB_W / 2;
        dl.fill(
            thumb_x as f32,
            self.y as f32,
            Self::THUMB_W as f32,
            self.h as f32,
            WHITE,
        );
    }
}

/// `UITextInput` : fond, texte échelle 2 à `(x + 8, y + 0,3·h)`, curseur `|` clignotant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInput {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub background: UiTexture,
    pub cursor_blink: f64,
}

impl TextInput {
    pub fn new(x: i32, y: i32, w: i32, h: i32, background: UiTexture) -> Self {
        Self {
            x,
            y,
            w,
            h,
            background,
            cursor_blink: 0.0,
        }
    }

    pub fn update(&mut self, dt: f64) {
        self.cursor_blink = (self.cursor_blink + dt) % 1.0;
    }

    pub fn draw(&self, dl: &mut DrawList, text: &str) {
        dl.image(
            self.background,
            self.x as f32,
            self.y as f32,
            self.w as f32,
            self.h as f32,
            WHITE,
        );
        let display = format!("{text}{}", if self.cursor_blink < 0.5 { "|" } else { "" });
        dl.text(
            &display,
            (self.x + 8) as f32,
            self.y as f32 + self.h as f32 * 0.3,
            2.0,
            WHITE,
        );
    }
}

/// Libellé centré : `x + (w - tw)/2`, `y + (h - 9·s)/2`.
pub fn centered_label(dl: &mut DrawList, label: &str, x: f32, y: f32, w: f32, h: f32, s: f32) {
    let tw = font::measure_width(label, s);
    dl.text(label, x + (w - tw) / 2.0, y + (h - 9.0 * s) / 2.0, s, WHITE);
}

/// `renderButton(x, y, w, h, label, enabled)` des écrans de menu (teinte 0,45 si inactif).
#[allow(clippy::too_many_arguments)]
pub fn button(dl: &mut DrawList, x: i32, y: i32, w: i32, h: i32, label: &str, enabled: bool) {
    let tint = if enabled { 1.0 } else { 0.45 };
    tinted_button(dl, x, y, w, h, label, tint);
}

/// `renderTabButton` (teinte 0,6 si inactif).
pub fn tab_button(dl: &mut DrawList, x: i32, y: i32, w: i32, h: i32, label: &str, active: bool) {
    let tint = if active { 1.0 } else { 0.60 };
    tinted_button(dl, x, y, w, h, label, tint);
}

fn tinted_button(dl: &mut DrawList, x: i32, y: i32, w: i32, h: i32, label: &str, tint: f32) {
    let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
    dl.image(UiTexture::Button, x, y, w, h, gray(tint));
    centered_label(dl, label, x, y, w, h, 2.0);
}

/// Texte centré horizontalement sur la fenêtre (`renderCentered`, `renderScreenTitle`).
pub fn centered_text(dl: &mut DrawList, text: &str, win_w: i32, y: f32, scale: f32, c: Rgba) {
    let tw = font::measure_width(text, scale);
    dl.text(text, win_w as f32 / 2.0 - tw / 2.0, y, scale, c);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_inclusive() {
        assert!(hit_test(10.0, 10.0, 10, 10, 5, 5));
        assert!(hit_test(15.0, 15.0, 10, 10, 5, 5));
        assert!(!hit_test(15.01, 12.0, 10, 10, 5, 5));
        assert!(!hit_test(9.99, 12.0, 10, 10, 5, 5));
    }

    #[test]
    fn vertical_layout_centers_and_stacks() {
        let mut p = Panel::new(0, 360, 1280, 360, Some(Layout::Vertical { spacing: 10 }));
        p.add(Element::button(400, 50, "PLAY", 1));
        p.add(Element::button(401, 50, "SETTINGS", 2));
        assert_eq!((p.children[0].x, p.children[0].y), (440, 360));
        // (1280 - 401) / 2 = 439 (division entière).
        assert_eq!((p.children[1].x, p.children[1].y), (439, 420));
    }

    #[test]
    fn grid_layout() {
        let mut p = Panel::new(
            28,
            294,
            567,
            189,
            Some(Layout::Grid {
                columns: 9,
                spacing_x: 7,
                spacing_y: 7,
            }),
        );
        for _ in 0..27 {
            p.add(Element::new(
                0,
                0,
                56,
                56,
                ElementKind::Slot {
                    highlight: None,
                    action: None,
                },
            ));
        }
        assert_eq!((p.children[8].x, p.children[8].y), (28 + 8 * 63, 294));
        assert_eq!((p.children[9].x, p.children[9].y), (28, 294 + 63));
        assert_eq!(
            (p.children[26].x, p.children[26].y),
            (28 + 8 * 63, 294 + 126)
        );
    }

    #[test]
    fn nested_click_collects_all_hovered() {
        let mut root = Panel::new(0, 0, 100, 100, None);
        let mut inner = Panel::new(0, 0, 100, 100, Some(Layout::Vertical { spacing: 0 }));
        inner.add(Element::button(100, 50, "A", 7));
        inner.add(Element::button(100, 50, "B", 8));
        root.add_panel(inner);
        // y = 50 : bornes incluses → les deux boutons.
        assert_eq!(root.handle_click(10.0, 50.0), vec![7, 8]);
        assert_eq!(root.handle_click(10.0, 10.0), vec![7]);
        assert!(root.handle_click(200.0, 10.0).is_empty());
    }

    #[test]
    fn slider_value_and_draw() {
        let mut s = Slider::new(280, 32, 4.0, 24.0, 30.0);
        assert_eq!(s.value, 24.0);
        s.set_position(100, 0);
        s.set_value_from_mouse_x(240.0);
        assert!((s.value - 14.0).abs() < 1e-5);
        s.set_value_from_mouse_x(-50.0);
        assert_eq!(s.value, 4.0);
        let mut dl = DrawList::new(800, 600);
        s.draw(&mut dl);
        assert_eq!(dl.quad_count(), 2, "valeur min : pas de remplissage");
        s.value = 24.0;
        let mut dl = DrawList::new(800, 600);
        s.draw(&mut dl);
        assert_eq!(dl.quad_count(), 3);
        // Poignée centrée sur x + w.
        assert_eq!(dl.vertices()[12].pos[0], (100 + 280 - 7) as f32);
    }

    #[test]
    fn slot_highlight_on_hover() {
        let e = Element::new(
            10,
            10,
            56,
            56,
            ElementKind::Slot {
                highlight: Some(UiTexture::White),
                action: None,
            },
        );
        let mut dl = DrawList::new(100, 100);
        e.draw(&mut dl, 0.0, 0.0);
        assert!(dl.is_empty());
        e.draw(&mut dl, 66.0, 66.0);
        assert_eq!(dl.quad_count(), 1);
        assert_eq!(dl.vertices()[0].color, [1.0, 1.0, 1.0, 0.4]);
    }

    #[test]
    fn text_button_label_position() {
        let mut dl = DrawList::new(1280, 720);
        let mut e = Element::button(400, 50, "PLAY", 0);
        e.set_position(440, 360);
        e.draw(&mut dl, 0.0, 0.0);
        // Premier sommet = texture du bouton ; libellé à x = 440 + (400 - 50)/2 = 615.
        assert_eq!(dl.vertices()[0].pos, [440.0, 360.0]);
        let min_x = dl.vertices()[6..]
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::MAX, f32::min);
        assert_eq!(min_x, 615.0);
    }

    #[test]
    fn text_input_cursor_blinks() {
        let mut t = TextInput::new(0, 0, 200, 40, UiTexture::White);
        let mut a = DrawList::new(10, 10);
        t.draw(&mut a, "ab");
        t.update(0.6);
        let mut b = DrawList::new(10, 10);
        t.draw(&mut b, "ab");
        assert!(a.quad_count() > b.quad_count());
    }
}
