//! Éléments d'interface d'une partie en réseau local qui se superposent au jeu :
//! liste des joueurs (maintenir Tab) et écran d'attente du terrain à l'arrivée.

use crate::draw::{gray, DrawList, Rgba, YELLOW};
use crate::font::measure_width;

const WHITE: Rgba = [1.0, 1.0, 1.0, 1.0];

/// Une ligne de la liste des joueurs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerRow {
    pub name: String,
    pub is_self: bool,
    pub is_host: bool,
    pub ping_ms: Option<u32>,
}

fn centered(dl: &mut DrawList, w: i32, y: f32, text: &str, scale: f32, color: Rgba) {
    let tw = measure_width(text, scale);
    dl.text(text, w as f32 / 2.0 - tw / 2.0, y, scale, color);
}

/// Liste des joueurs, au centre haut de l'écran (maintenir Tab).
pub fn draw_player_list(dl: &mut DrawList, w: i32, _h: i32, rows: &[PlayerRow]) {
    const ROW_H: f32 = 30.0;
    let panel_w = 460.0f32.min(w as f32 - 20.0);
    let panel_h = 56.0 + rows.len() as f32 * ROW_H + 10.0;
    let x = w as f32 / 2.0 - panel_w / 2.0;
    let y = 40.0;
    dl.fill(x, y, panel_w, panel_h, [0.0, 0.0, 0.0, 0.65]);
    centered(
        dl,
        w,
        y + 12.0,
        &format!("Players ({})", rows.len()),
        2.4,
        YELLOW,
    );
    for (i, r) in rows.iter().enumerate() {
        let ry = y + 52.0 + i as f32 * ROW_H;
        let mut label = r.name.clone();
        if r.is_host {
            label.push_str(" (host)");
        }
        if r.is_self {
            label.push_str(" (you)");
        }
        let color = if r.is_self { YELLOW } else { WHITE };
        dl.text(&label, x + 16.0, ry, 2.2, color);
        if let Some(ms) = r.ping_ms {
            let t = format!("{ms} ms");
            let tw = measure_width(&t, 2.0);
            dl.text(&t, x + panel_w - 16.0 - tw, ry + 2.0, 2.0, gray(0.8));
        }
    }
}

/// Voile d'attente : le client a rejoint la partie mais le terrain arrive encore.
pub fn draw_terrain_loading(dl: &mut DrawList, w: i32, h: i32, progress: Option<(usize, usize)>) {
    dl.fill(0.0, 0.0, w as f32, h as f32, [0.0, 0.0, 0.0, 0.75]);
    let cy = h as f32 / 2.0;
    centered(dl, w, cy - 40.0, "Joining the game...", 3.0, WHITE);
    let (got, total) = progress.unwrap_or((0, 1));
    centered(
        dl,
        w,
        cy + 4.0,
        &format!("Receiving the world from the host: {got} / {total} chunks"),
        1.8,
        gray(0.85),
    );
    let bar_w = 360.0f32.min(w as f32 - 40.0);
    let bx = w as f32 / 2.0 - bar_w / 2.0;
    dl.fill(bx, cy + 40.0, bar_w, 14.0, [0.15, 0.15, 0.15, 1.0]);
    let frac = if total == 0 {
        0.0
    } else {
        (got as f32 / total as f32).clamp(0.0, 1.0)
    };
    dl.fill(bx, cy + 40.0, bar_w * frac, 14.0, [0.3, 0.8, 0.3, 1.0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_draw_something() {
        let mut dl = DrawList::new(1280, 720);
        draw_player_list(
            &mut dl,
            1280,
            720,
            &[
                PlayerRow {
                    name: "Lea".into(),
                    is_self: false,
                    is_host: true,
                    ping_ms: None,
                },
                PlayerRow {
                    name: "Tom".into(),
                    is_self: true,
                    is_host: false,
                    ping_ms: Some(12),
                },
            ],
        );
        let n = dl.batches().len();
        assert!(n > 0);
        let mut dl = DrawList::new(1280, 720);
        draw_terrain_loading(&mut dl, 1280, 720, Some((10, 49)));
        draw_terrain_loading(&mut dl, 1280, 720, None);
        assert!(!dl.batches().is_empty());
    }
}
