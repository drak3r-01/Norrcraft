//! Automate cellulaire de l'eau (doc 07 §3) : file de cellules éveillées, règles R1 à R5,
//! recherche de pression, sommeil et régime permanent.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use nb_core::glam::IVec3;
use nb_core::ChunkPos;
use nb_data::{BlockId, WaterRules};

use super::blocks::{FluidBlocks, FluidClass};
use super::layer::{Dir, FluidCode, FULL};
use super::table::{WaterRest, WaterTable};
use super::{FluidDelta, FluidEnv, FluidEvents, FluidFx, FluidWorld, SplitMix64};

/// Capacité d'une cellule infinie.
const INF_CAP: i32 = 1 << 20;
pub(super) const SIDES: [IVec3; 4] = [
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 0, -1),
];
const NEIGH6: [IVec3; 6] = [
    IVec3::new(1, 0, 0),
    IVec3::new(-1, 0, 0),
    IVec3::new(0, 0, 1),
    IVec3::new(0, 0, -1),
    IVec3::new(0, 1, 0),
    IVec3::new(0, -1, 0),
];
const Y: IVec3 = IVec3::new(0, 1, 0);

#[derive(Clone, Copy, Debug)]
pub(super) struct CellMeta {
    pub hist: [u8; 2],
    pub calm: u8,
    pub flux: bool,
    /// La cellule a fait tomber de l'eau pendant ce pas.
    pub down: bool,
    pub out: Dir,
}

impl Default for CellMeta {
    fn default() -> Self {
        Self {
            hist: [255, 255],
            calm: 0,
            flux: false,
            down: false,
            out: Dir::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpringKind {
    Mountain,
    Karst,
    Glacier,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Emitter {
    pub kind: SpringKind,
    pub acc: f32,
}

/// Statistiques de la simulation (`/water info`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FluidStats {
    /// Cellules en file (étape en cours et suivante).
    pub awake: usize,
    /// Cellules traitées pendant le dernier tick.
    pub processed: u32,
    /// Durée du dernier tick, en microsecondes.
    pub last_tick_us: u32,
    /// Pas d'eau écoulés depuis la création.
    pub steps: u64,
    /// Octets de couches fluides (renseigné par l'appelant).
    pub layer_bytes: usize,
}

pub struct FluidSim {
    pub(super) rules: Arc<WaterRules>,
    pub(super) blocks: FluidBlocks,
    pub(super) cur: VecDeque<IVec3>,
    pub(super) next: Vec<IVec3>,
    pub(super) queued: HashSet<IVec3>,
    pub(super) received: HashMap<IVec3, u8>,
    /// Partenaires d'échange de chaque cellule pendant le pas en cours.
    links: HashMap<IVec3, Vec<IVec3>>,
    pub(super) meta: HashMap<IVec3, CellMeta>,
    tables: RefCell<HashMap<ChunkPos, Arc<WaterTable>>>,
    pub(super) emitters: HashMap<IVec3, Emitter>,
    /// Chunks à remailler en fin de pas ; vrai si une cellule de bord a changé.
    dirty: HashMap<ChunkPos, bool>,
    deltas: Vec<FluidDelta>,
    collect_deltas: bool,
    fx: Vec<FluidFx>,
    pub(super) step: u64,
    step_tick: u32,
    searches_left: u32,
    pub(super) rng: SplitMix64,
    /// Chunks des joueurs ; vide = tout est simulé (tests).
    centers: Vec<(i32, i32)>,
    has_tables: bool,
    pub stats: FluidStats,
}

impl std::fmt::Debug for FluidSim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FluidSim")
            .field("step", &self.step)
            .field("awake", &(self.cur.len() + self.next.len()))
            .finish_non_exhaustive()
    }
}

#[inline]
fn chunk_of(p: IVec3) -> ChunkPos {
    ChunkPos::new(p.x.div_euclid(16), p.z.div_euclid(16))
}

fn hash_cell(p: IVec3, step: u64) -> u64 {
    crate::norrcraft::math::hash64(step, p.x, p.y, p.z)
}

impl FluidSim {
    /// `tables` : le monde fournit des tables d'eau (nappes infinies) ; `seed` : graine du monde.
    pub fn new(rules: Arc<WaterRules>, blocks: FluidBlocks, seed: u64, tables: bool) -> Self {
        Self {
            rules,
            blocks,
            cur: VecDeque::new(),
            next: Vec::new(),
            queued: HashSet::new(),
            received: HashMap::new(),
            links: HashMap::new(),
            meta: HashMap::new(),
            tables: RefCell::new(HashMap::new()),
            emitters: HashMap::new(),
            dirty: HashMap::new(),
            deltas: Vec::new(),
            collect_deltas: false,
            fx: Vec::new(),
            step: 0,
            step_tick: 0,
            searches_left: 0,
            rng: SplitMix64(seed ^ 0x4E52_5743_0000_0801),
            centers: Vec::new(),
            has_tables: tables,
            stats: FluidStats::default(),
        }
    }

    pub fn rules(&self) -> &Arc<WaterRules> {
        &self.rules
    }

    pub fn blocks(&self) -> &FluidBlocks {
        &self.blocks
    }

    /// Les tables d'eau sont-elles disponibles ?
    pub fn has_tables(&self) -> bool {
        self.has_tables
    }

    /// Cellules en file.
    pub fn awake(&self) -> usize {
        self.cur.len() + self.next.len()
    }

    /// La file est-elle vide (eau au repos) ?
    pub fn is_idle(&self) -> bool {
        self.cur.is_empty() && self.next.is_empty()
    }

    pub fn set_collect_deltas(&mut self, on: bool) {
        self.collect_deltas = on;
        if !on {
            self.deltas.clear();
        }
    }

    pub fn take_deltas(&mut self) -> Vec<FluidDelta> {
        std::mem::take(&mut self.deltas)
    }

    pub fn take_fx(&mut self) -> Vec<FluidFx> {
        std::mem::take(&mut self.fx)
    }

    /// Centres de simulation (joueurs), en coordonnées de monde.
    pub fn set_centers(&mut self, centers: &[nb_core::glam::Vec3]) {
        self.centers = centers
            .iter()
            .map(|c| {
                (
                    (c.x / 16.0).floor() as i32,
                    (c.z / 16.0).floor() as i32,
                )
            })
            .collect();
    }

    // ------------------------------------------------------------------ lectures

    /// Ligne de table d'eau de la colonne `(x, z)`.
    pub fn rest(&self, w: &dyn FluidWorld, x: i32, z: i32) -> Option<WaterRest> {
        if !self.has_tables {
            return None;
        }
        let cp = ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
        let idx = (z.rem_euclid(16) * 16 + x.rem_euclid(16)) as usize;
        let mut tables = self.tables.borrow_mut();
        if let Some(t) = tables.get(&cp) {
            return Some(t.0[idx]);
        }
        let t = w.table(cp)?;
        let r = t.0[idx];
        if tables.len() > 1024 {
            tables.clear();
        }
        tables.insert(cp, t);
        Some(r)
    }

    pub(super) fn info(
        &self,
        w: &dyn FluidWorld,
        p: IVec3,
    ) -> Option<(BlockId, FluidCode, FluidClass)> {
        w.cell(p).map(|(id, c)| (id, c, self.blocks.class_of(id)))
    }

    /// Cellule dans une nappe infinie (le voxel doit être ouvert, vérifié par l'appelant).
    pub fn inf(&self, w: &dyn FluidWorld, p: IVec3) -> bool {
        self.has_tables
            && self
                .rest(w, p.x, p.z)
                .is_some_and(|r| r.infinite(p.y))
    }

    /// Niveau courant 0..=8 (une nappe infinie ouverte vaut 8).
    pub fn lvl(&self, w: &dyn FluidWorld, p: IVec3) -> i32 {
        let Some((_, code, class)) = self.info(w, p) else {
            return 0;
        };
        match class {
            FluidClass::Closed => 0,
            FluidClass::Empty | FluidClass::Washable => {
                if self.inf(w, p) {
                    FULL as i32
                } else {
                    0
                }
            }
            FluidClass::Water => {
                if self.inf(w, p) {
                    FULL as i32
                } else {
                    i32::from(code.level(FULL))
                }
            }
            FluidClass::Permeable => {
                if self.inf(w, p) {
                    FULL as i32
                } else {
                    i32::from(code.level(0))
                }
            }
            FluidClass::Aquatic => {
                if self.inf(w, p) {
                    FULL as i32
                } else {
                    i32::from(code.level(FULL))
                }
            }
        }
    }

    pub fn open(&self, w: &dyn FluidWorld, p: IVec3) -> bool {
        self.info(w, p).is_some_and(|(_, _, c)| c != FluidClass::Closed)
    }

    pub(super) fn cap(&self, w: &dyn FluidWorld, p: IVec3) -> i32 {
        match self.info(w, p) {
            None => 0,
            Some((_, _, FluidClass::Closed)) => 0,
            Some(_) => {
                if self.inf(w, p) {
                    INF_CAP
                } else {
                    FULL as i32 - self.lvl(w, p)
                }
            }
        }
    }

    pub(super) fn head(&self, w: &dyn FluidWorld, p: IVec3) -> i32 {
        8 * p.y + self.lvl(w, p)
    }

    /// La cellule du dessous est fermée ou pleine (ou absente).
    pub(super) fn supported(&self, w: &dyn FluidWorld, p: IVec3) -> bool {
        let d = p - Y;
        // Une cellule infinie compte pour pleine (`lvl` vaut 8) : on repose sur la mer.
        !self.open(w, d) || self.lvl(w, d) >= FULL as i32
    }

    /// L'eau qui entre dans `n` tombera. Une cellule de nappe infinie n'est jamais un bord.
    pub(super) fn is_edge(&self, w: &dyn FluidWorld, n: IVec3) -> bool {
        let d = n - Y;
        self.open(w, d) && self.cap(w, d) > 0 && !self.inf(w, n)
    }

    pub(super) fn in_radius(&self, p: IVec3) -> bool {
        if self.centers.is_empty() {
            return true;
        }
        let c = chunk_of(p);
        let r = self.rules.sim_radius;
        self.centers
            .iter()
            .any(|&(x, z)| (c.x - x).abs() <= r && (c.z - z).abs() <= r)
    }

    // ------------------------------------------------------------------ écriture

    fn code_for(&self, id: BlockId, level: i32) -> FluidCode {
        let l = level.clamp(0, FULL as i32) as u8;
        if l == self.blocks.default_level(id) {
            FluidCode::DEFAULT
        } else {
            FluidCode::explicit(l, Dir::None, false)
        }
    }

    pub(super) fn write(&mut self, w: &mut dyn FluidWorld, p: IVec3, id: BlockId, code: FluidCode) {
        if let Some((oid, ocode)) = w.cell(p) {
            if oid == id && ocode == code {
                return;
            }
        } else {
            return;
        }
        w.write(p, id, code);
        let border = p.x.rem_euclid(16) == 0
            || p.x.rem_euclid(16) == 15
            || p.z.rem_euclid(16) == 0
            || p.z.rem_euclid(16) == 15;
        let e = self.dirty.entry(chunk_of(p)).or_insert(false);
        *e |= border;
        if self.collect_deltas {
            self.deltas.push(FluidDelta {
                pos: p,
                block: id,
                code: code.0,
            });
        }
    }

    /// Fixe le niveau d'une cellule ouverte ; remplace l'air ou un bloc emporté par de l'eau.
    pub(super) fn set_level(&mut self, w: &mut dyn FluidWorld, p: IVec3, level: i32) {
        let Some((id, _, class)) = self.info(w, p) else {
            return;
        };
        let level = level.clamp(0, FULL as i32);
        match class {
            FluidClass::Closed => {}
            FluidClass::Water => {
                if level == 0 {
                    self.write(w, p, self.blocks.air, FluidCode::DEFAULT);
                } else {
                    let c = self.code_for(id, level);
                    self.write(w, p, id, c);
                }
            }
            FluidClass::Empty | FluidClass::Washable => {
                if level > 0 {
                    let wid = self.blocks.water;
                    let c = self.code_for(wid, level);
                    self.write(w, p, wid, c);
                }
            }
            FluidClass::Permeable | FluidClass::Aquatic => {
                let c = self.code_for(id, level);
                self.write(w, p, id, c);
            }
        }
    }

    // ------------------------------------------------------------------ réveils

    /// Met `p` dans la file du pas suivant ; efface son régime permanent.
    pub(super) fn wake(&mut self, w: &mut dyn FluidWorld, p: IVec3) {
        if self.queued.insert(p) {
            self.next.push(p);
        }
        if let Some((id, code)) = w.cell(p) {
            if code.steady() {
                self.write(w, p, id, code.calm());
            }
        }
    }

    /// Réveille `p` et ses six voisines qui portent de l'eau ou sont infinies.
    pub fn wake_around(&mut self, w: &mut dyn FluidWorld, p: IVec3) {
        self.wake(w, p);
        for d in NEIGH6 {
            let n = p + d;
            if self.lvl(w, n) > 0 {
                self.wake(w, n);
            }
        }
    }

    fn wake_water_neighbors(&mut self, w: &mut dyn FluidWorld, p: IVec3) {
        for d in NEIGH6 {
            let n = p + d;
            if !self.queued.contains(&n) && self.lvl(w, n) > 0 {
                self.wake(w, n);
            }
        }
    }

    // ------------------------------------------------------------------ transfert

    /// Déplace jusqu'à `t` huitièmes de `a` vers `b`. Renvoie la quantité déplacée.
    fn transfer(&mut self, w: &mut dyn FluidWorld, a: IVec3, b: IVec3, t: i32, dir: Dir) -> i32 {
        let inf_a = self.inf(w, a);
        let inf_b = self.inf(w, b);
        if inf_a && inf_b {
            return 0; // deux nappes infinies : rien à déplacer
        }
        let mut t = t;
        if !inf_a {
            t = t.min(self.lvl(w, a));
        }
        if !inf_b {
            t = t.min(FULL as i32 - self.lvl(w, b));
        }
        if t <= 0 || !self.open(w, b) {
            return 0;
        }
        if !inf_a {
            let la = self.lvl(w, a);
            self.set_level(w, a, la - t);
        }
        if !inf_b {
            let lb = self.lvl(w, b);
            self.set_level(w, b, lb + t);
            *self.received.entry(b).or_insert(0) += t as u8;
        }
        let ma = self.meta.entry(a).or_default();
        ma.flux = true;
        ma.out = dir;
        if dir == Dir::Down {
            ma.down = true;
        }
        self.meta.entry(b).or_default().flux = true;
        self.links.entry(a).or_default().push(b);
        self.links.entry(b).or_default().push(a);
        self.wake(w, a);
        self.wake(w, b);
        self.wake_water_neighbors(w, a);
        self.wake_water_neighbors(w, b);
        if !inf_b {
            self.quench(w, b, t);
        }
        t
    }

    /// R5. Trempe : eau contre lave.
    fn quench(&mut self, w: &mut dyn FluidWorld, b: IVec3, t: i32) {
        let lava = self.blocks.lava;
        for d in NEIGH6 {
            let n = b + d;
            if w.cell(n).is_some_and(|(id, _)| id == lava) {
                let rock = self.blocks.quench;
                self.write(w, n, rock, FluidCode::DEFAULT);
                let lb = self.lvl(w, b);
                self.set_level(w, b, lb - t);
                self.fx.push(FluidFx::Quench(n));
                self.wake_around(w, b);
                return;
            }
        }
    }

    // ------------------------------------------------------------------ une cellule

    fn update_cell(&mut self, w: &mut dyn FluidWorld, p: IVec3) {
        self.queued.remove(&p);
        if !self.in_radius(p) {
            return;
        }
        let Some((_, _, class)) = self.info(w, p) else {
            return;
        };
        if class == FluidClass::Closed {
            return;
        }
        let inf_p = self.inf(w, p);
        let mut avail;
        if inf_p {
            self.set_level(w, p, FULL as i32);
            avail = FULL as i32;
        } else {
            avail = self.lvl(w, p) - i32::from(self.received.get(&p).copied().unwrap_or(0));
        }
        if avail <= 0 {
            return;
        }
        let mut moved = 0;

        // R1. Chute.
        let d = p - Y;
        if self.open(w, d) {
            let cd = self.cap(w, d);
            if cd > 0 {
                let m = self.transfer(w, p, d, avail.min(cd), Dir::Down);
                moved += m;
                avail -= m;
            }
        }
        if avail <= 0 {
            return;
        }

        // R2. Étalement (cellule soutenue).
        if self.supported(w, p) {
            let rot = (self.step.wrapping_add(hash_cell(p, 0)) % 4) as usize;
            let mut cand: Vec<IVec3> = Vec::with_capacity(4);
            for i in 0..4 {
                let n = p + SIDES[(i + rot) % 4];
                if self.open(w, n) {
                    cand.push(n);
                }
            }
            let mut budget = avail.min(i32::from(self.rules.spread_max));
            while budget > 0 && !cand.is_empty() {
                let mut best = cand[0];
                let mut best_eff = i32::MAX;
                for &n in &cand {
                    let eff = if self.is_edge(w, n) {
                        -8
                    } else {
                        self.lvl(w, n)
                    };
                    if eff < best_eff {
                        best_eff = eff;
                        best = n;
                    }
                }
                let n = best;
                let mine = self.lvl(w, p);
                let cn = self.cap(w, n);
                let ok = if self.is_edge(w, n) || inf_p {
                    cn > 0
                } else {
                    mine - self.lvl(w, n) >= 2 && cn > 0
                };
                if !ok {
                    break;
                }
                let dir = dir_to(p, n);
                let m = self.transfer(w, p, n, 1, dir);
                if m == 0 {
                    break;
                }
                moved += m;
                avail -= m;
                budget -= m;
            }
        }

        // R3. Égalisation à distance et pression.
        if moved == 0 && avail > 0 && self.supported(w, p) && self.lvl(w, p) >= 2 {
            if self.searches_left > 0 {
                self.searches_left -= 1;
                if let Some(t) = self.pressure_target(w, p) {
                    let diff = self.head(w, p) - self.head(w, t);
                    let amount = avail.min(self.cap(w, t)).min((diff / 2).max(1));
                    let m = self.transfer(w, p, t, amount, Dir::None);
                    moved += m;
                }
            } else {
                // Budget de recherches épuisé : on réessaie au pas suivant.
                self.wake(w, p);
            }
        }

        // R4. Ruissellement d'un film.
        if moved == 0 && self.lvl(w, p) == 1 && self.supported(w, p) {
            if let Some(n) = self.seek_slope(w, p) {
                let dir = dir_to(p, n);
                self.transfer(w, p, n, 1, dir);
            }
        }
    }

    /// R3 : cellule la plus basse atteignable (surface plane, vases communicants).
    fn pressure_target(&self, w: &dyn FluidWorld, s: IVec3) -> Option<IVec3> {
        let hs = self.head(w, s);
        let nodes = self.rules.pressure_nodes as usize;
        let mut best: Option<(IVec3, i32)> = None;
        let mut queue = VecDeque::from([s]);
        let mut seen: HashSet<IVec3> = HashSet::from([s]);
        let mut visits = 0usize;
        while let Some(c) = queue.pop_front() {
            if visits >= nodes {
                break;
            }
            visits += 1;
            for d in [
                IVec3::new(0, -1, 0),
                IVec3::new(1, 0, 0),
                IVec3::new(-1, 0, 0),
                IVec3::new(0, 0, 1),
                IVec3::new(0, 0, -1),
                Y,
            ] {
                let v = c + d;
                if v.y > s.y || !seen.insert(v) {
                    continue;
                }
                if !self.open(w, v) || self.inf(w, v) {
                    continue;
                }
                let hv = self.head(w, v);
                if self.cap(w, v) > 0 && hv + 2 <= hs && best.is_none_or(|(_, hb)| hv < hb) {
                    best = Some((v, hv));
                }
                let l = self.lvl(w, v);
                if l == FULL as i32 || (v.y == c.y && l >= 1) {
                    queue.push_back(v);
                }
            }
        }
        best.map(|(v, _)| v)
    }

    /// R4 : premier pas du plus court chemin d'un film vers un bord ou une pente.
    fn seek_slope(&self, w: &dyn FluidWorld, p: IVec3) -> Option<IVec3> {
        let radius = self.rules.seek_radius;
        let mut queue: VecDeque<(IVec3, IVec3, i32)> = VecDeque::new();
        let mut seen: HashSet<IVec3> = HashSet::from([p]);
        for s in SIDES {
            let n = p + s;
            if self.open(w, n) && seen.insert(n) {
                queue.push_back((n, n, 1));
            }
        }
        while let Some((c, first, dist)) = queue.pop_front() {
            let l = self.lvl(w, c);
            if l > 1 {
                continue;
            }
            if self.is_edge(w, c) {
                return Some(first);
            }
            let hc = self.head(w, c);
            if SIDES.iter().any(|s| {
                let n = c + *s;
                self.open(w, n) && self.head(w, n) + 2 <= hc
            }) {
                return Some(first);
            }
            if dist >= radius || !self.supported(w, c) {
                continue;
            }
            for s in SIDES {
                let n = c + s;
                if self.open(w, n) && seen.insert(n) {
                    queue.push_back((n, first, dist + 1));
                }
            }
        }
        None
    }

    // ------------------------------------------------------------------ tick

    /// Un tick de jeu (doc 07 §3.3).
    pub fn tick(&mut self, w: &mut dyn FluidWorld, events: FluidEvents, env: &FluidEnv) {
        let t0 = Instant::now();
        self.absorb_events(w, events);
        self.searches_left = self.rules.pressure_searches_per_tick;
        let mut n = 0u32;
        let cap = self.rules.cells_per_tick;
        let cap_us = self.rules.time_cap_micros;
        while n < cap {
            let Some(p) = self.cur.pop_front() else {
                break;
            };
            self.update_cell(w, p);
            n += 1;
            if cap_us > 0 && n.is_multiple_of(64) && t0.elapsed().as_micros() as u64 > cap_us {
                break;
            }
        }
        self.step_tick += 1;
        if self.cur.is_empty() && self.step_tick >= self.rules.step_ticks {
            self.end_step(w, env);
        }
        self.stats.awake = self.cur.len() + self.next.len();
        self.stats.processed = n;
        self.stats.steps = self.step;
        self.stats.last_tick_us = t0.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
    }

    /// Fin de pas : sommeil, régime permanent, sources, remaillage.
    fn end_step(&mut self, w: &mut dyn FluidWorld, env: &FluidEnv) {
        let mut pending: Vec<IVec3> = std::mem::take(&mut self.next);
        // De bas en haut : la colonne d'eau qui tombe reste continue d'un pas à l'autre.
        pending.sort_by_key(|p| (p.y, p.x, p.z));
        let calm_steps = self.rules.calm_steps;
        // Calme : le niveau de fin de pas reprend l'une des deux valeurs précédentes.
        let mut levels: HashMap<IVec3, u8> = HashMap::with_capacity(pending.len());
        for &p in &pending {
            let l = self.lvl(w, p) as u8;
            levels.insert(p, l);
            let m = self.meta.entry(p).or_default();
            if l == m.hist[0] || l == m.hist[1] {
                m.calm = m.calm.saturating_add(1);
            } else {
                m.calm = 0;
            }
            m.hist = [l, m.hist[0]];
        }
        // Candidats au régime permanent : une cellule traversée par un flux constant qui tombe
        // (cascade), ou une nappe infinie qui l'alimente. Les écoulements horizontaux d'eau libre
        // restent simulés tant qu'ils bougent : figer une pente en cours d'équilibrage la ferait
        // dormir hors équilibre. Une cellule ne s'installe que si tous ses partenaires d'échange
        // du pas sont eux aussi candidats (ou déjà au repos) : une source ne se retire pas tant
        // que l'aval bouge encore.
        let mut steady: HashSet<IVec3> = pending
            .iter()
            .copied()
            .filter(|&p| {
                self.meta.get(&p).is_some_and(|m| {
                    m.calm >= calm_steps && m.flux && (m.down || self.inf(w, p))
                })
            })
            .collect();
        loop {
            let drop: Vec<IVec3> = steady
                .iter()
                .copied()
                .filter(|p| {
                    self.links.get(p).is_some_and(|ls| {
                        ls.iter()
                            .any(|q| self.queued.contains(q) && !steady.contains(q))
                    })
                })
                .collect();
            if drop.is_empty() {
                break;
            }
            for p in drop {
                steady.remove(&p);
            }
        }
        for p in pending {
            let l = levels[&p];
            if steady.contains(&p) {
                let out = self
                    .meta
                    .get(&p)
                    .map_or(Dir::None, |m| if m.down { Dir::Down } else { m.out });
                self.meta.remove(&p);
                self.queued.remove(&p);
                if l > 0 && !self.inf(w, p) {
                    if let Some((id, _, class)) = self.info(w, p) {
                        if class != FluidClass::Closed {
                            self.write(w, p, id, FluidCode::explicit(l, out, true));
                        }
                    }
                }
            } else {
                if let Some(m) = self.meta.get_mut(&p) {
                    m.flux = false;
                    m.down = false;
                }
                self.cur.push_back(p);
            }
        }
        self.links.clear();
        self.received.clear();
        let queued = &self.queued;
        self.meta.retain(|p, _| queued.contains(p));
        self.sources_step(w, env);
        let dirty: Vec<(ChunkPos, bool)> = self.dirty.drain().collect();
        for (c, border) in dirty {
            w.mark_mesh(c);
            if border {
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    w.mark_mesh(ChunkPos::new(c.x + dx, c.z + dz));
                }
            }
        }
        self.step += 1;
        self.step_tick = 0;
    }

    // ------------------------------------------------------------------ événements

    fn absorb_events(&mut self, w: &mut dyn FluidWorld, ev: FluidEvents) {
        for p in ev.edits {
            self.wake(w, p);
            for d in NEIGH6 {
                let n = p + d;
                if self.lvl(w, n) > 0 || self.inf(w, n) {
                    self.wake(w, n);
                }
            }
        }
        for c in ev.unloaded {
            self.cur.retain(|p| chunk_of(*p) != c);
            self.next.retain(|p| chunk_of(*p) != c);
            self.queued.retain(|p| chunk_of(*p) != c);
            self.meta.retain(|p, _| chunk_of(*p) != c);
            self.emitters.retain(|p, _| chunk_of(*p) != c);
            self.tables.borrow_mut().remove(&c);
            self.dirty.remove(&c);
        }
        for c in ev.loaded {
            self.boot_chunk(w, c);
        }
    }

    fn boot_chunk(&mut self, w: &mut dyn FluidWorld, c: ChunkPos) {
        if self.has_tables {
            if let Some(t) = w.table(c) {
                self.tables.borrow_mut().insert(c, t);
            }
        }
        let mut explicit = Vec::new();
        let mut springs = Vec::new();
        w.scan(c, self.blocks.spring, &mut explicit, &mut springs);
        for p in springs {
            let kind = self.spring_kind(w, p);
            self.emitters.insert(p, Emitter { kind, acc: 0.0 });
        }
        let mut woken = 0u32;
        for (p, code) in explicit {
            if !code.steady() && woken < self.rules.boot_wake_max {
                self.wake(w, p);
                woken += 1;
            }
        }
        // L'eau qui attendait au bord d'un chunk voisin repart.
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nc = ChunkPos::new(c.x + dx, c.z + dz);
            let mut ex = Vec::new();
            let mut sp = Vec::new();
            w.scan(nc, self.blocks.spring, &mut ex, &mut sp);
            for (p, code) in ex {
                let on_face = match (dx, dz) {
                    (1, 0) => p.x.rem_euclid(16) == 0,
                    (-1, 0) => p.x.rem_euclid(16) == 15,
                    (0, 1) => p.z.rem_euclid(16) == 0,
                    _ => p.z.rem_euclid(16) == 15,
                };
                if on_face && !code.steady() {
                    self.wake(w, p);
                }
            }
        }
    }

    // ------------------------------------------------------------------ seau et commandes

    /// Verse `amount` h dans `p` ; renvoie la quantité versée.
    pub fn pour(&mut self, w: &mut dyn FluidWorld, p: IVec3, amount: u8) -> u8 {
        if !self.open(w, p) {
            return 0;
        }
        let t = i32::from(amount).min(self.cap(w, p));
        if t <= 0 {
            return 0;
        }
        if self.inf(w, p) {
            return amount;
        }
        let l = self.lvl(w, p);
        self.set_level(w, p, l + t);
        self.wake_around(w, p);
        t as u8
    }

    /// Prend 8 h dans la cellule (pleine ou infinie).
    pub fn scoop(&mut self, w: &mut dyn FluidWorld, p: IVec3) -> bool {
        if !self.open(w, p) {
            return false;
        }
        if self.inf(w, p) {
            return true;
        }
        if self.lvl(w, p) < FULL as i32 {
            return false;
        }
        self.set_level(w, p, 0);
        self.wake_around(w, p);
        true
    }

    /// Pose un bloc `kalla` en `p` et l'enregistre comme émetteur (commande `/water spring`).
    pub fn place_spring(&mut self, w: &mut dyn FluidWorld, p: IVec3) -> bool {
        let id = self.blocks.spring;
        if id == nb_data::UNASSIGNED_ID || w.cell(p).is_none() {
            return false;
        }
        self.write(w, p, id, FluidCode::DEFAULT);
        let kind = self.spring_kind(w, p);
        self.emitters.insert(p, Emitter { kind, acc: 0.0 });
        self.wake_around(w, p);
        true
    }

    /// Retire toute l'eau libre dans un rayon (commande `/water drain`).
    pub fn drain_radius(&mut self, w: &mut dyn FluidWorld, c: IVec3, r: i32) -> u32 {
        let mut n = 0;
        for x in -r..=r {
            for y in -r..=r {
                for z in -r..=r {
                    let p = c + IVec3::new(x, y, z);
                    if self.lvl(w, p) > 0 && !self.inf(w, p) {
                        self.set_level(w, p, 0);
                        self.wake_around(w, p);
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// Réveille toute l'eau d'un rayon (commande `/water wake`).
    pub fn wake_radius(&mut self, w: &mut dyn FluidWorld, c: IVec3, r: i32) -> u32 {
        let mut n = 0;
        for x in -r..=r {
            for y in -r..=r {
                for z in -r..=r {
                    let p = c + IVec3::new(x, y, z);
                    if self.lvl(w, p) > 0 {
                        self.wake(w, p);
                        n += 1;
                    }
                }
            }
        }
        n
    }

    /// Volume total (huitièmes) de l'eau finie d'un volume ; sert aux tests.
    pub fn total_volume(&self, w: &dyn FluidWorld, min: IVec3, max: IVec3) -> i64 {
        let mut s = 0i64;
        for x in min.x..max.x {
            for y in min.y..max.y {
                for z in min.z..max.z {
                    let p = IVec3::new(x, y, z);
                    if !self.inf(w, p) {
                        s += i64::from(self.lvl(w, p));
                    }
                }
            }
        }
        s
    }
}

fn dir_to(a: IVec3, b: IVec3) -> Dir {
    let d = b - a;
    if d.y < 0 {
        Dir::Down
    } else {
        Dir::horizontal(d.x, d.z)
    }
}
