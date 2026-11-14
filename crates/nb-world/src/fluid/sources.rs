//! Sources, suintements, pluie, évaporation, gel et recharge des lacs (doc 07 §4).

use nb_core::glam::IVec3;

use super::blocks::FluidClass;
use super::layer::{Dir, FluidCode, FULL};
use super::sim::{Emitter, FluidSim, SpringKind};
use super::table::RestKind;
use super::{FluidEnv, FluidWorld};

const Y: IVec3 = IVec3::new(0, 1, 0);
/// Rayon (en blocs) autour d'un joueur où les colonnes de météo sont tirées.
const SAMPLE_RADIUS: i32 = 64;
const SKY_Y: i32 = 127;

impl FluidSim {
    /// Roche voisine d'une source : `kalksten` → karst, glace → glacier, sinon montagne.
    pub(super) fn spring_kind(&self, w: &dyn FluidWorld, p: IVec3) -> SpringKind {
        let mut kind = SpringKind::Mountain;
        for d in [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Z,
            IVec3::NEG_Z,
            IVec3::Y,
            IVec3::NEG_Y,
        ] {
            if let Some((id, _)) = w.cell(p + d) {
                if id == self.blocks.glacier_ice {
                    return SpringKind::Glacier;
                }
                if id == self.blocks.limestone {
                    kind = SpringKind::Karst;
                }
            }
        }
        kind
    }

    /// La cellule fait-elle partie d'une nappe générée (mer, rivière, lac) ?
    fn in_sheet(&self, w: &dyn FluidWorld, p: IVec3) -> bool {
        self.rest(w, p.x, p.z).is_some_and(|r| {
            r.kind != RestKind::Dry && p.y >= i32::from(r.top) && p.y < i32::from(r.level)
        })
    }

    /// Premier bloc « sol » en descendant du ciel : `(y, classe)`. Les blocs emportés et l'air
    /// sont traversés.
    fn skyline(&self, w: &dyn FluidWorld, x: i32, z: i32) -> Option<i32> {
        let mut y = SKY_Y;
        while y >= 0 {
            match self.info(w, IVec3::new(x, y, z)) {
                None => return None,
                Some((_, _, FluidClass::Empty | FluidClass::Washable)) => y -= 1,
                Some(_) => return Some(y),
            }
        }
        None
    }

    fn sample_column(&mut self, env: &FluidEnv) -> Option<(i32, i32)> {
        if env.centers.is_empty() {
            return None;
        }
        let c = env.centers[self.rng.below(env.centers.len() as u32) as usize];        let x = c.x.floor() as i32 + self.rng.around(SAMPLE_RADIUS);
        let z = c.z.floor() as i32 + self.rng.around(SAMPLE_RADIUS);
        Some((x, z))
    }

    pub(super) fn sources_step(&mut self, w: &mut dyn FluidWorld, env: &FluidEnv) {
        self.emit(w, env);
        let wr = self.rules.weather.clone();
        if wr.steps > 0 && self.step.is_multiple_of(u64::from(wr.steps)) {
            for _ in 0..wr.rain_samples {
                if let Some((x, z)) = self.sample_column(env) {
                    self.weather_sample(w, env, x, z);
                }
            }
        }
        if wr.recharge_steps > 0 && self.step.is_multiple_of(u64::from(wr.recharge_steps)) {
            for _ in 0..wr.recharge_samples {
                if let Some((x, z)) = self.sample_column(env) {
                    self.recharge_sample(w, env, x, z);
                }
            }
        }
        if wr.seep_steps > 0 && self.step.is_multiple_of(u64::from(wr.seep_steps)) {
            for _ in 0..wr.seep_samples {
                if let Some((x, z)) = self.sample_column(env) {
                    self.seep_sample(w, env, x, z);
                }
            }
        }
    }

    /// 4.2. Sources `kalla`.
    fn emit(&mut self, w: &mut dyn FluidWorld, env: &FluidEnv) {
        if self.emitters.is_empty() {
            return;
        }
        let season = env.season_step.map_or(4, |s| usize::from(s.min(7)));
        let keys: Vec<IVec3> = self.emitters.keys().copied().collect();
        let mut keys = keys;
        keys.sort_by_key(|p| (p.y, p.x, p.z));
        for p in keys {
            if !self.in_radius(p) || w.cell(p).is_none() {
                continue;
            }
            let Some(em) = self.emitters.get(&p).copied() else {
                continue;
            };
            let rule = match em.kind {
                SpringKind::Mountain => &self.rules.springs.mountain,
                SpringKind::Karst => &self.rules.springs.karst,
                SpringKind::Glacier => &self.rules.springs.glacier,
            };
            let mut acc = em.acc + rule.rate * rule.season[season];
            if acc < 1.0 {
                // La source ne verse rien ce pas : un écoulement en régime permanent qu'elle
                // alimentait doit se remettre à couler (ou à se vider).
                for d in [Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
                    let n = p + d;
                    if w.cell(n).is_some_and(|(_, c)| c.steady()) {
                        self.wake(w, n);
                    }
                }
            }
            while acc >= 1.0 {
                let mut placed = false;
                for d in [Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
                    let t = p + d;
                    if self.open(w, t) && self.cap(w, t) > 0 && !self.inf(w, t) {
                        let l = self.lvl(w, t);
                        self.set_level(w, t, l + 1);
                        self.wake_around(w, t);
                        placed = true;
                        break;
                    }
                }
                if !placed {
                    acc = acc.min(4.0);
                    break;
                }
                acc -= 1.0;
            }
            if let Some(e) = self.emitters.get_mut(&p) {
                *e = Emitter { acc, ..em };
            }
        }
    }

    /// 4.4 et 4.5. Pluie, flaques, évaporation, gel et dégel de l'eau libre.
    fn weather_sample(&mut self, w: &mut dyn FluidWorld, env: &FluidEnv, x: i32, z: i32) {
        let Some(gy) = self.skyline(w, x, z) else {
            return;
        };
        let g = IVec3::new(x, gy, z);
        let Some((gid, _, gclass)) = self.info(w, g) else {
            return;
        };
        let wr = self.rules.weather.clone();
        let raining = env.rain > 0.3 && env.air_temp > 0.0;
        let free = !self.in_sheet(w, g) && !self.queued.contains(&g);
        // Dégel de la glace libre.
        if env.air_temp > wr.thaw_above && free {
            if gid == self.blocks.new_ice {
                self.write(w, g, self.blocks.water, FluidCode::DEFAULT);
                self.wake_around(w, g);
                return;
            }
            if gid == self.blocks.aufeis {
                self.write(w, g, self.blocks.water, FluidCode::explicit(2, Dir::None, false));
                self.wake_around(w, g);
                return;
            }
        }
        if gclass == FluidClass::Water && free {
            let code = w.cell(g).map_or(FluidCode::DEFAULT, |c| c.1);
            let l = self.lvl(w, g);
            // Gel de l'eau libre au repos, à ciel ouvert.
            if env.air_temp < wr.freeze_below && !code.steady() && l > 0 {
                let ice = if l == FULL as i32 {
                    self.blocks.new_ice
                } else {
                    self.blocks.aufeis
                };
                if ice != nb_data::UNASSIGNED_ID {
                    self.write(w, g, ice, FluidCode::DEFAULT);
                    for d in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
                        let n = g + d;
                        if self.lvl(w, n) > 0 {
                            self.wake(w, n);
                        }
                    }
                    return;
                }
            }
            if raining && l < i32::from(wr.puddle_max) {
                self.set_level(w, g, l + 1);
                self.wake(w, g);
                return;
            }
            // Évaporation.
            if !raining && env.daylight && env.air_temp > 0.0 && (1..=2).contains(&l) && !code.steady()
            {
                self.set_level(w, g, l - 1);
                self.wake(w, g);
            }
            return;
        }
        // Flaque dans un creux.
        if raining {
            let p = g + Y;
            if !self.in_sheet(w, p) && self.lvl(w, p) < i32::from(wr.puddle_max) && self.open(w, p) {
                let hollow = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                    .iter()
                    .all(|d| {
                        self.skyline(w, x + d.x, z + d.z)
                            .is_some_and(|ny| ny >= gy)
                    });
                if hollow && gclass != FluidClass::Empty {
                    let l = self.lvl(w, p);
                    self.set_level(w, p, l + 1);
                    self.wake(w, p);
                }
            }
        }
    }

    /// 4.1. Recharge lente des lacs.
    fn recharge_sample(&mut self, w: &mut dyn FluidWorld, env: &FluidEnv, x: i32, z: i32) {
        let Some(r) = self.rest(w, x, z) else {
            return;
        };
        if r.kind != RestKind::Lake || r.level == 0 {
            return;
        }
        let c = IVec3::new(x, i32::from(r.level) - 1, z);
        let below = c - Y;
        if self.open(w, c)
            && !self.inf(w, c)
            && self.lvl(w, c) < FULL as i32
            && (!self.open(w, below) || self.lvl(w, below) == FULL as i32)
        {
            let add = if env.rain > 0.3 { 2 } else { 1 };
            let l = self.lvl(w, c);
            self.set_level(w, c, (l + add).min(FULL as i32));
            self.wake_around(w, c);
        }
    }

    /// 4.3. Suintement des tourbières.
    fn seep_sample(&mut self, w: &mut dyn FluidWorld, env: &FluidEnv, x: i32, z: i32) {
        let Some(gy) = self.skyline(w, x, z) else {
            return;
        };
        let g = IVec3::new(x, gy, z);
        if w.cell(g).map(|c| c.0) != Some(self.blocks.seep) {
            return;
        }
        let thaw = matches!(env.season_step, Some(2 | 3));
        if !(env.rain > 0.3 || thaw) {
            return;
        }
        for d in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            let n = g + d;
            if self.open(w, n) && self.cap(w, n) > 0 && !self.inf(w, n) {
                let l = self.lvl(w, n);
                self.set_level(w, n, l + 1);
                self.wake_around(w, n);
                return;
            }
        }
    }
}
