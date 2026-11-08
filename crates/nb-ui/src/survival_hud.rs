//! Interface du mode survie : barres de faim / chaleur / endurance, armure, progression du
//! minage et des repas, pêche, messages, butin récent, barres d'usure, infobulles d'objets.

use crate::draw::{DrawList, Rgba, WHITE};
use crate::font;
use crate::hud::{HotbarLayout, HUD_ICON_GAP, HUD_ICON_SIZE, HOTBAR_W};
use crate::textures::UiTexture;

/// Caractéristiques d'un outil, pour l'infobulle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolView {
    pub label: &'static str,
    pub tier: &'static str,
    pub damage: f32,
    pub speed: f32,
}

/// Données descriptives d'un objet (infobulle, barre d'usure).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ItemInfoView {
    pub tool: Option<ToolView>,
    /// (faim, saturation, chaleur)
    pub food: Option<(f32, f32, f32)>,
    /// (défense, isolation en %)
    pub armor: Option<(f32, f32)>,
    /// (usure, durabilité max)
    pub wear: Option<(u16, u16)>,
}

impl ItemInfoView {
    /// Fraction de durabilité restante (None si l'objet ne s'use pas ou est neuf).
    pub fn durability_left(&self) -> Option<f32> {
        let (w, m) = self.wear?;
        (m > 0 && w > 0).then(|| 1.0 - f32::from(w) / f32::from(m))
    }

    /// Lignes de l'infobulle sous le nom.
    pub fn lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(t) = self.tool {
            v.push(format!("{} {}", t.tier, t.label));
            if t.label != "Fishing rod" {
                v.push(format!("Damage {:.1}   Speed x{:.1}", t.damage, t.speed));
            }
        }
        if let Some((h, s, w)) = self.food {
            v.push(format!("Food +{h:.0}   Saturation +{s:.0}"));
            if w > 0.0 {
                v.push(format!("Warms +{w:.0}"));
            }
        }
        if let Some((d, w)) = self.armor {
            v.push(format!("Armor {d:.0}   Warmth +{w:.0}%"));
        }
        if let Some((w, m)) = self.wear {
            if m > 0 {
                v.push(format!("Durability {}/{}", m.saturating_sub(w), m));
            }
        }
        v
    }
}

/// État du joueur vu par le HUD.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurvivalHud<'a> {
    /// 0..20
    pub hunger: f32,
    /// 0..100
    pub warmth: f32,
    /// 0..100
    pub stamina: f32,
    pub exhausted: bool,
    /// Points de défense de l'armure portée.
    pub defense: f32,
    /// Température ressentie (négatif = froid).
    pub felt: f32,
    pub mining: Option<f32>,
    pub eating: Option<f32>,
    /// `Some(true)` quand un poisson mord.
    pub fishing: Option<bool>,
    /// Messages (texte, secondes restantes).
    pub notices: &'a [(String, f32)],
    /// Butin récent (nom, quantité, secondes restantes).
    pub pickups: &'a [(String, i32, f32)],
}

/// Barre d'usure sous une icône (fraction restante 0..1).
pub fn durability_bar(dl: &mut DrawList, x: f32, y: f32, size: f32, left: f32) {
    let w = size - 4.0;
    let h = (size / 14.0).max(2.0);
    let by = y + size - h - 1.0;
    dl.fill(x + 2.0, by, w, h, [0.0, 0.0, 0.0, 0.85]);
    let f = left.clamp(0.0, 1.0);
    let col = if f > 0.5 {
        [1.0 - (f - 0.5) * 2.0, 0.9, 0.15, 1.0]
    } else {
        [0.95, f * 1.8, 0.1, 1.0]
    };
    dl.fill(x + 2.0, by, w * f, h, col);
}

const SHIELD: [&str; 10] = [
    "1111111111",
    "1111111111",
    "1111111111",
    "1111111111",
    "1111111111",
    "0111111110",
    "0111111110",
    "0011111100",
    "0001111000",
    "0000110000",
];

/// Bouclier d'armure (`fill` 0 = vide, 1 = demi, 2 = plein).
fn shield(dl: &mut DrawList, x: f32, y: f32, s: f32, fill: u8) {
    let c = s / 10.0;
    for (r, row) in SHIELD.iter().enumerate() {
        let b = row.as_bytes();
        let mut start = None;
        for i in 0..=10 {
            let on = i < 10 && b[i] == b'1';
            match (start, on) {
                (None, true) => start = Some(i),
                (Some(a), false) => {
                    let (px, w) = (x + a as f32 * c, (i - a) as f32 * c);
                    let col: Rgba = [0.18, 0.2, 0.26, 0.75];
                    dl.fill(px, y + r as f32 * c, w, c, col);
                    let lit: Rgba = [0.78, 0.82, 0.92, 1.0];
                    match fill {
                        2 => dl.fill(px, y + r as f32 * c, w, c, lit),
                        1 => dl.fill(px, y + r as f32 * c, w.min((5.0 - a as f32).max(0.0) * c), c, lit),
                        _ => {}
                    }
                    start = None;
                }
                _ => {}
            }
        }
    }
}

/// Barre horizontale étiquetée.
fn bar(dl: &mut DrawList, x: f32, y: f32, w: f32, h: f32, f: f32, col: Rgba, label: &str) {
    dl.fill(x - 2.0, y - 2.0, w + 4.0, h + 4.0, [0.0, 0.0, 0.0, 0.7]);
    dl.fill(x, y, w, h, [0.12, 0.12, 0.14, 0.9]);
    dl.fill(x, y, w * f.clamp(0.0, 1.0), h, col);
    dl.fill(x, y, w * f.clamp(0.0, 1.0), h * 0.4, [1.0, 1.0, 1.0, 0.18]);
    dl.text(label, x + 4.0, y + (h - 10.0 * 1.2) / 2.0, 1.2, [1.0, 1.0, 1.0, 0.95]);
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        1.0,
    ]
}

/// Dessin complet du HUD de survie (au-dessus de la barre rapide).
pub fn draw(dl: &mut DrawList, l: &HotbarLayout, win_w: i32, win_h: i32, s: &SurvivalHud<'_>) {
    let step = HUD_ICON_SIZE + HUD_ICON_GAP;
    let hud_y = l.hotbar_y as f32 - HUD_ICON_SIZE - 4.0;
    // Faim : 10 icônes, de droite à gauche.
    let total = 10.0 * step - HUD_ICON_GAP;
    let start = l.hotbar_x as f32 + HOTBAR_W as f32 - total;
    let per = s.hunger / 2.0;
    for i in 0..10 {
        let rem = per - (9 - i) as f32;
        let tex = if rem >= 1.0 {
            UiTexture::HungerFull
        } else if rem >= 0.5 {
            UiTexture::HungerHalf
        } else {
            UiTexture::HungerEmpty
        };
        dl.image(tex, start + i as f32 * step, hud_y, HUD_ICON_SIZE, HUD_ICON_SIZE, WHITE);
    }
    // Armure : 10 boucliers (2 points de défense chacun, max 20) au-dessus des cœurs.
    if s.defense > 0.0 {
        let y = hud_y - HUD_ICON_SIZE * 0.9 - 3.0;
        for i in 0..10 {
            let rem = s.defense / 2.0 - i as f32;
            let f = if rem >= 1.0 {
                2
            } else if rem >= 0.5 {
                1
            } else {
                0
            };
            shield(
                dl,
                l.hotbar_x as f32 + i as f32 * step,
                y,
                HUD_ICON_SIZE * 0.9,
                f,
            );
        }
    }
    // Chaleur et endurance au-dessus de la faim (à droite).
    let bw = 150.0;
    let bx = l.hotbar_x as f32 + HOTBAR_W as f32 - bw;
    let by = hud_y - 26.0;
    let w = (s.warmth / 100.0).clamp(0.0, 1.0);
    let wcol = if s.warmth < 30.0 {
        lerp3([0.35, 0.6, 1.0], [0.55, 0.85, 1.0], s.warmth / 30.0)
    } else {
        lerp3([0.95, 0.62, 0.2], [0.98, 0.45, 0.15], (s.warmth - 30.0) / 70.0)
    };
    let wlabel = if s.warmth < 10.0 {
        "FREEZING"
    } else if s.warmth < 30.0 {
        "Cold"
    } else if s.felt > 0.35 {
        "Hot"
    } else {
        "Warmth"
    };
    bar(dl, bx, by, bw, 16.0, w, wcol, wlabel);
    let st = (s.stamina / 100.0).clamp(0.0, 1.0);
    let scol = if s.exhausted {
        [0.8, 0.25, 0.2, 1.0]
    } else {
        lerp3([0.9, 0.8, 0.2], [0.35, 0.85, 0.3], st)
    };
    bar(dl, bx, by - 22.0, bw, 12.0, st, scol, if s.exhausted { "Exhausted" } else { "Stamina" });

    let (cx, cy) = (win_w as f32 / 2.0, win_h as f32 / 2.0);
    // Progression du minage / d'un repas, sous le viseur.
    let prog = s.mining.or(s.eating);
    if let Some(p) = prog {
        let (pw, ph) = (90.0, 8.0);
        dl.fill(cx - pw / 2.0 - 2.0, cy + 30.0 - 2.0, pw + 4.0, ph + 4.0, [0.0, 0.0, 0.0, 0.6]);
        let col = if s.eating.is_some() {
            [0.95, 0.75, 0.3, 1.0]
        } else {
            [0.9, 0.9, 0.9, 1.0]
        };
        dl.fill(cx - pw / 2.0, cy + 30.0, pw * p.clamp(0.0, 1.0), ph, col);
    }
    // Pêche.
    if let Some(bite) = s.fishing {
        let (text, col) = if bite {
            ("!  BITE - right click to reel in  !", [1.0, 0.9, 0.2, 1.0])
        } else {
            ("Fishing... wait for a bite", [0.8, 0.9, 1.0, 0.9])
        };
        let sc = 2.0;
        let tw = font::measure_width(text, sc);
        dl.text(text, cx - tw / 2.0, cy - 90.0, sc, col);
    }
    // Messages au-dessus de la barre de vie.
    let mut y = hud_y - 80.0;
    for (text, left) in s.notices.iter().rev().take(4) {
        let a = (*left / 0.6).clamp(0.0, 1.0);
        let sc = 1.8;
        let tw = font::measure_width(text, sc);
        let x = cx - tw / 2.0;
        dl.fill(x - 8.0, y - 4.0, tw + 16.0, 10.0 * sc + 8.0, [0.0, 0.0, 0.0, 0.55 * a]);
        dl.text(text, x, y, sc, [1.0, 0.95, 0.75, a]);
        y -= 10.0 * sc + 12.0;
    }
    // Butin récent à droite.
    let mut y = win_h as f32 * 0.42;
    for (name, n, left) in s.pickups.iter().rev().take(6) {
        let a = (*left / 0.6).clamp(0.0, 1.0);
        let text = format!("+{n} {name}");
        let sc = 1.8;
        let tw = font::measure_width(&text, sc);
        let x = win_w as f32 - tw - 18.0;
        dl.fill(x - 8.0, y - 3.0, tw + 16.0, 10.0 * sc + 6.0, [0.0, 0.0, 0.0, 0.5 * a]);
        dl.text(&text, x, y, sc, [0.85, 1.0, 0.85, a]);
        y += 10.0 * sc + 10.0;
    }
    // Givre sur les bords de l'écran quand on gèle.
    if s.warmth < 40.0 {
        let k = ((40.0 - s.warmth) / 40.0).clamp(0.0, 1.0);
        let (w, h) = (win_w as f32, win_h as f32);
        for i in 0..8 {
            let t = i as f32 / 8.0;
            let a = k * 0.28 * (1.0 - t);
            let d = 6.0 + t * 90.0 * (0.4 + k);
            let col: Rgba = [0.75, 0.9, 1.0, a];
            dl.fill(0.0, 0.0, w, d, col);
            dl.fill(0.0, h - d, w, d, col);
            dl.fill(0.0, d, d, h - 2.0 * d, col);
            dl.fill(w - d, d, d, h - 2.0 * d, col);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_lines_describe_tools_food_and_wear() {
        let i = ItemInfoView {
            tool: Some(ToolView {
                label: "Pickaxe",
                tier: "Iron",
                damage: 3.5,
                speed: 5.5,
            }),
            wear: Some((50, 250)),
            ..Default::default()
        };
        let l = i.lines();
        assert_eq!(l[0], "Iron Pickaxe");
        assert!(l[1].contains("Damage 3.5"));
        assert_eq!(l[2], "Durability 200/250");
        assert!((i.durability_left().unwrap() - 0.8).abs() < 1e-6);
        assert!(ItemInfoView::default().lines().is_empty());
    }

    #[test]
    fn hud_draws_something() {
        let mut dl = DrawList::new(1280, 720);
        let l = HotbarLayout::new(1280, 720);
        let s = SurvivalHud {
            hunger: 13.0,
            warmth: 20.0,
            stamina: 50.0,
            defense: 7.0,
            mining: Some(0.4),
            ..Default::default()
        };
        draw(&mut dl, &l, 1280, 720, &s);
        assert!(dl.quad_count() > 30);
    }
}
