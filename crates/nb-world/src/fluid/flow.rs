//! Courants (doc 07 §6.2) et crochet pour l'eau motrice (§6.7).

use nb_core::glam::{IVec3, Vec3};

use super::layer::Dir;
use super::sim::{FluidSim, SIDES};
use super::table::RestKind;
use super::FluidWorld;

const Y: IVec3 = IVec3::new(0, 1, 0);

impl FluidSim {
    /// L'eau de `p` tombe-t-elle ?
    pub fn falling(&self, w: &dyn FluidWorld, p: IVec3) -> bool {
        if self.lvl(w, p) == 0 {
            return false;
        }
        let steady_down = w.cell(p).is_some_and(|(_, c)| c.dir() == Dir::Down);
        let d = p - Y;
        steady_down || (self.open(w, d) && self.cap(w, d) > 0)
    }

    /// Courant (blocs par seconde) dans la cellule `p` ; nul hors de l'eau.
    pub fn current_at(&self, w: &dyn FluidWorld, p: IVec3) -> Vec3 {
        let l = self.lvl(w, p);
        if l == 0 {
            return Vec3::ZERO;
        }
        let sp = &self.rules().speeds;
        if self.falling(w, p) {
            return Vec3::new(0.0, -sp.fall, 0.0);
        }
        let code = w.cell(p).map_or(Default::default(), |c| c.1);
        let mut v: Vec3;
        let dir = code.dir();
        if code.steady() && dir.is_horizontal() {
            let (dx, _, dz) = dir.vector();
            let speed = if l <= 3 { sp.film } else { sp.stream };
            v = Vec3::new(dx as f32, 0.0, dz as f32) * speed;
        } else if let Some(r) = self
            .rest(w, p.x, p.z)
            .filter(|r| r.kind == RestKind::River && r.infinite(p.y))
        {
            v = Vec3::new(f32::from(r.flow[0]), 0.0, f32::from(r.flow[1])) / 16.0;
        } else {
            let here = self.head(w, p);
            let h = |q: IVec3| {
                if self.lvl(w, q) > 0 {
                    self.head(w, q)
                } else {
                    here
                }
            };
            let gx = h(p - IVec3::X) - h(p + IVec3::X);
            let gz = h(p - IVec3::Z) - h(p + IVec3::Z);
            v = Vec3::new(gx as f32, 0.0, gz as f32) * sp.gradient;
            let len = v.length();
            if len > sp.max_free {
                v *= sp.max_free / len;
            }
        }
        let up = p + Y;
        if self.lvl(w, up) > 0 && self.falling(w, up) {
            v.y -= sp.plunge_pull;
        } else {
            for s in SIDES {
                let n = p + s;
                if self.lvl(w, n + Y) > 0 && self.falling(w, n + Y) {
                    let r = Vec3::new(s.x as f32, 0.0, s.z as f32);
                    v += Vec3::new(-r.z, 0.0, r.x) * sp.swirl;
                }
            }
        }
        v
    }

    /// Puissance disponible pour une roue en `p` : |courant horizontal| × niveau / 8, plus
    /// 2 × le débit d'une chute qui tombe sur `p + Y`.
    pub fn flow_power(&self, w: &dyn FluidWorld, p: IVec3) -> f32 {
        let c = self.current_at(w, p);
        let horizontal = Vec3::new(c.x, 0.0, c.z).length();
        let mut power = horizontal * self.lvl(w, p) as f32 / 8.0;
        let up = p + Y;
        if self.lvl(w, up) > 0 && self.falling(w, up) {
            power += 2.0 * self.lvl(w, up) as f32 / 8.0 * self.rules().speeds.fall / 6.0;
        }
        power
    }
}
