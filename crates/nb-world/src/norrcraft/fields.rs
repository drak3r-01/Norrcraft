//! Champs macroscopiques du monde Norrcraft (GDD doc 02 §4).
//!
//! Onze champs lents (continentalité, océanité, température, humidité, surrection, érosion,
//! densité de lacs, géothermie, province rocheuse, variante, sens d'écoulement des glaces)
//! décrivent la géographie à l'échelle de plusieurs kilomètres. Ils ne dépendent que de
//! (graine, configuration, x, z) : le générateur, le mailleur, le spawn et la survie voient les
//! mêmes valeurs, quel que soit l'ordre dans lequel les chunks sont demandés.
//!
//! Évaluation exacte : [`MacroFields::sample`]. Évaluation par chunk, sur un treillis de 4 × 4
//! blocs interpolé bilinéairement (doc 02 §4, « Optimisation ») : [`ChunkFields`].
//!
//! Orientation du préréglage Norrland : l'ouest (−x) est l'océan, l'est (+x) le golfe, le nord
//! (−z) le froid, le sud (+z) la douceur. Les biais saturent au-delà de ~9000 blocs : le monde
//! reste infini.

use super::config::NorrConfig;
use super::math::{clamp01, lerp, sigmoid, smooth, spread, GradField, Worley};

/// Constantes de semence des champs (`graine XOR constante`). Valeurs arbitraires, figées : les
/// changer change tous les mondes.
mod seeds {
    pub const WARP: u64 = 0x4E52_5743_0000_0001;
    pub const C: u64 = 0x4E52_5743_0000_0002;
    pub const O: u64 = 0x4E52_5743_0000_0003;
    pub const T0: u64 = 0x4E52_5743_0000_0004;
    pub const HM: u64 = 0x4E52_5743_0000_0005;
    pub const U: u64 = 0x4E52_5743_0000_0006;
    pub const E: u64 = 0x4E52_5743_0000_0007;
    pub const LK: u64 = 0x4E52_5743_0000_0008;
    pub const GT: u64 = 0x4E52_5743_0000_0009;
    pub const PV: u64 = 0x4E52_5743_0000_000A;
    pub const VR: u64 = 0x4E52_5743_0000_000B;
    pub const THETA: u64 = 0x4E52_5743_0000_000C;
}

/// Paramètres de la déformation de domaine commune `p' = warp(p; 120, 700)`.
pub const WARP_AMPLITUDE: f64 = 120.0;
pub const WARP_WAVELENGTH: f64 = 700.0;

/// Isoligne de C qui sert de trait de côte (doc 02 §5.1 : C ≈ 0,465).
pub const COAST_C: f64 = 0.465;

/// Pas du treillis de [`ChunkFields`], en blocs.
pub const LATTICE_STEP: i32 = 4;

/// Les champs en un point (colonne). Tous dans [0, 1] sauf `theta` (vecteur unitaire).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroSample {
    /// Continentalité : 0 = haute mer, 1 = cœur des terres. Le trait de côte est C ≈ 0,465.
    pub c: f64,
    /// Océanité : 1 = façade atlantique (douce, humide), 0 = façade baltique (saumâtre, gel).
    pub o: f64,
    /// Température au niveau de la mer : 0 = arctique, 1 = sud doux.
    pub t0: f64,
    /// Humidité : tourbières contre landes sèches.
    pub hm: f64,
    /// Surrection : où se dresse la chaîne.
    pub u: f64,
    /// Érosion : 1 = plat, 0 = accidenté.
    pub e: f64,
    /// Densité de lacs.
    pub lk: f64,
    /// Géothermie : province de lave (îles du large).
    pub gt: f64,
    /// Variante : sous-types locaux.
    pub vr: f64,
    /// Sens d'écoulement des glaces : vecteur unitaire (x, z).
    pub theta: [f64; 2],
}

impl MacroSample {
    /// Vrai si la colonne est au-dessus du trait de côte (C ≥ 0,465).
    pub fn is_land(&self) -> bool {
        self.c >= COAST_C
    }

    /// Combinaison bilinéaire de quatre échantillons (`tx`, `tz` dans [0, 1]).
    fn bilinear(s00: &Self, s10: &Self, s01: &Self, s11: &Self, tx: f64, tz: f64) -> Self {
        let m = |a: f64, b: f64, c: f64, d: f64| lerp(lerp(a, b, tx), lerp(c, d, tx), tz);
        let tx_ = m(s00.theta[0], s10.theta[0], s01.theta[0], s11.theta[0]);
        let tz_ = m(s00.theta[1], s10.theta[1], s01.theta[1], s11.theta[1]);
        let norm = (tx_ * tx_ + tz_ * tz_).sqrt();
        let theta = if norm > 1e-9 {
            [tx_ / norm, tz_ / norm]
        } else {
            s00.theta
        };
        Self {
            c: m(s00.c, s10.c, s01.c, s11.c),
            o: m(s00.o, s10.o, s01.o, s11.o),
            t0: m(s00.t0, s10.t0, s01.t0, s11.t0),
            hm: m(s00.hm, s10.hm, s01.hm, s11.hm),
            u: m(s00.u, s10.u, s01.u, s11.u),
            e: m(s00.e, s10.e, s01.e, s11.e),
            lk: m(s00.lk, s10.lk, s01.lk, s11.lk),
            gt: m(s00.gt, s10.gt, s01.gt, s11.gt),
            vr: m(s00.vr, s10.vr, s01.vr, s11.vr),
            theta,
        }
    }
}

/// Provinces rocheuses du socle (doc 02 §8.1), une par cellule de Worley de pas 1400.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Province {
    /// Granit gris (50 %).
    GraniteGrey,
    /// Gneiss rubané (20 %, obligatoire dans la chaîne).
    Gneiss,
    /// Granit rouge (8 %) : côte soulevée.
    GraniteRed,
    /// Calcaire (8 %, sud-est et Alvar).
    Limestone,
    /// Grès (5 %).
    Sandstone,
    /// Schiste alunifère (4 %, combustible).
    AlumShale,
    /// Grönsten, roche verte minéralisée (4 %) : pays minier.
    Greenstone,
    /// Porphyre (1 %, décoratif).
    Porphyry,
}

impl Province {
    pub const ALL: [Province; 8] = [
        Self::GraniteGrey,
        Self::Gneiss,
        Self::GraniteRed,
        Self::Limestone,
        Self::Sandstone,
        Self::AlumShale,
        Self::Greenstone,
        Self::Porphyry,
    ];

    /// Part de la surface visée (en %).
    pub fn share(self) -> f64 {
        match self {
            Self::GraniteGrey => 50.0,
            Self::Gneiss => 20.0,
            Self::GraniteRed => 8.0,
            Self::Limestone => 8.0,
            Self::Sandstone => 5.0,
            Self::AlumShale => 4.0,
            Self::Greenstone => 4.0,
            Self::Porphyry => 1.0,
        }
    }

    /// Nom suédois (identifiant du jeu).
    pub fn name(self) -> &'static str {
        match self {
            Self::GraniteGrey => "grå granit",
            Self::Gneiss => "gnejs",
            Self::GraniteRed => "röd granit",
            Self::Limestone => "kalksten",
            Self::Sandstone => "sandsten",
            Self::AlumShale => "alunskiffer",
            Self::Greenstone => "grönsten",
            Self::Porphyry => "porfyr",
        }
    }
}

/// Évaluateur des champs macroscopiques d'un monde (graine + configuration).
///
/// Cloner est peu coûteux (quelques ko) : chaque thread de génération peut avoir le sien.
#[derive(Debug, Clone)]
pub struct MacroFields {
    cfg: NorrConfig,
    warp: GradField,
    c: GradField,
    o: GradField,
    t0: GradField,
    hm: GradField,
    u: GradField,
    e: GradField,
    lk: GradField,
    gt: GradField,
    vr: GradField,
    pv: Worley,
    theta: Worley,
}

impl MacroFields {
    pub fn new(seed: i64, cfg: NorrConfig) -> Self {
        Self {
            cfg,
            warp: GradField::new(seed, seeds::WARP),
            c: GradField::new(seed, seeds::C),
            o: GradField::new(seed, seeds::O),
            t0: GradField::new(seed, seeds::T0),
            hm: GradField::new(seed, seeds::HM),
            u: GradField::new(seed, seeds::U),
            e: GradField::new(seed, seeds::E),
            lk: GradField::new(seed, seeds::LK),
            gt: GradField::new(seed, seeds::GT),
            vr: GradField::new(seed, seeds::VR),
            pv: Worley::new(seed, seeds::PV, 1400.0),
            theta: Worley::new(seed, seeds::THETA, 3000.0),
        }
    }

    pub fn config(&self) -> &NorrConfig {
        &self.cfg
    }

    /// `p' = warp(p; 120, 700)` : coordonnées déformées communes à C, T₀, Hm, U, E et Pv.
    pub fn warped(&self, x: f64, z: f64) -> (f64, f64) {
        self.warp.warp(x, z, WARP_AMPLITUDE, WARP_WAVELENGTH)
    }

    /// Biais d'océanité du préréglage Norrland : 1 à l'ouest, 0 à l'est, `0,5 − 0,5 · s(x / 4000)`.
    pub fn ocean_bias(x: f64) -> f64 {
        0.5 - 0.5 * sigmoid(x / 4000.0)
    }

    /// Biais de chaleur du préréglage Norrland : 0 au nord (−z), 1 au sud, `0,5 + 0,5 · s(z / 3500)`.
    pub fn warmth_bias(z: f64) -> f64 {
        0.5 + 0.5 * sigmoid(z / 3500.0)
    }

    /// Nœuds `(x, décalage de C)` du biais d'océanité du préréglage Norrland : océan profond à
    /// l'ouest (−0,40), noyau continental surélevé (+0,17) pour que les terres du centre
    /// ne soient pas criblées de mers intérieures, golfe à l'est (−0,05, puis contraction vers le
    /// plateau continental, voir `GULF_PULL`). Constant au-delà
    /// des extrémités (le monde reste infini). Entre deux nœuds : seuil lisse (dérivée nulle aux nœuds).
    const C_BIAS_KNOTS: [(f64, f64); 7] = [
        (-9000.0, -0.40),
        (-6500.0, -0.18),
        (-4500.0, 0.08),
        (-2000.0, 0.17),
        (2500.0, 0.17),
        (5500.0, 0.02),
        (9000.0, -0.05),
    ];

    /// Poids du golfe oriental : 0 à l'ouest de x ≈ 3500, 1 à l'est de x ≈ 8000.
    pub fn gulf_weight(x: f64) -> f64 {
        smooth(3500.0, 8000.0, x)
    }

    /// Vers quelle valeur de C le golfe contracte la continentalité (plateau continental, juste sous
    /// la côte) et avec quelle force : mer peu profonde (la Baltique), îles là où le bruit est haut.
    const GULF_TARGET_C: f64 = 0.42;
    const GULF_PULL: f64 = 0.55;

    /// Décalage de C selon x (voir `C_BIAS_KNOTS`).
    pub fn continentality_bias(x: f64) -> f64 {
        let k = &Self::C_BIAS_KNOTS;
        if x <= k[0].0 {
            return k[0].1;
        }
        for w in k.windows(2) {
            if x <= w[1].0 {
                return lerp(w[0].1, w[1].1, smooth(w[0].0, w[1].0, x));
            }
        }
        k[k.len() - 1].1
    }

    /// Océanité O en (x, z).
    pub fn ocean(&self, x: f64, z: f64) -> f64 {
        let noise = spread(self.o.fbm(x / 5000.0, z / 5000.0, 2));
        clamp01(0.6 * Self::ocean_bias(x) + 0.4 * noise)
    }

    /// Province rocheuse dans la cellule de Worley (coordonnées déformées).
    fn province_warped(&self, px: f64, pz: f64) -> Province {
        let hit = self.pv.eval(px, pz);
        let r = self.pv.cell_rand(hit.cell.0, hit.cell.1, 0);
        // Le calcaire se concentre au sud-est (+x, +z) ; les parts restent celles du doc en moyenne.
        let se = smooth(-4000.0, 4000.0, hit.center[0] + hit.center[1]);
        let mut weights = [0.0; 8];
        for (w, p) in weights.iter_mut().zip(Province::ALL) {
            *w = p.share();
        }
        weights[Province::Limestone as usize] *= 0.2 + 1.6 * se;
        let total: f64 = weights.iter().sum();
        let mut acc = 0.0;
        for (w, p) in weights.iter().zip(Province::ALL) {
            acc += w / total;
            if r < acc {
                return p;
            }
        }
        Province::GraniteGrey
    }

    /// Province rocheuse du socle en (x, z).
    pub fn province(&self, x: f64, z: f64) -> Province {
        let (px, pz) = self.warped(x, z);
        self.province_warped(px, pz)
    }

    /// Identifiant de la cellule de province (pour distinguer deux provinces voisines de même nature).
    pub fn province_cell(&self, x: f64, z: f64) -> (i32, i32) {
        let (px, pz) = self.warped(x, z);
        self.pv.eval(px, pz).cell
    }

    /// Sens d'écoulement des glaces θ : vecteur unitaire constant par cellule de Worley de pas
    /// 3000, mélangé continûment avec la cellule voisine près des frontières.
    pub fn flow_direction(&self, x: f64, z: f64) -> [f64; 2] {
        let unit = |i: i32, j: i32| -> [f64; 2] {
            let a = 2.0 * self.theta.cell_rand(i, j, 1) - 1.0;
            let b = 2.0 * self.theta.cell_rand(i, j, 2) - 1.0;
            let n = (a * a + b * b).sqrt();
            if n > 1e-6 {
                [a / n, b / n]
            } else {
                [1.0, 0.0]
            }
        };
        let pitch = self.theta.pitch();
        let nearest = self.theta.eval(x, z);
        let (ci, cj) = ((x / pitch).floor() as i32, (z / pitch).floor() as i32);
        // Moyenne pondérée des vecteurs des cellules voisines : le poids d'une cellule s'annule
        // continûment quand son point s'éloigne de plus de 0,25 pas au-delà du plus proche.
        let reach = 0.25 * pitch;
        let (mut sx, mut sz) = (0.0, 0.0);
        for dj in -1..=1 {
            for di in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                let p = self.theta.point(i, j);
                let (dx, dz) = (p[0] - x, p[1] - z);
                let d = (dx * dx + dz * dz).sqrt();
                let w = clamp01(1.0 - (d - nearest.f1) / reach);
                let w = w * w;
                if w > 0.0 {
                    let v = unit(i, j);
                    sx += w * v[0];
                    sz += w * v[1];
                }
            }
        }
        let n = (sx * sx + sz * sz).sqrt();
        if n > 1e-9 {
            [sx / n, sz / n]
        } else {
            unit(nearest.cell.0, nearest.cell.1)
        }
    }

    /// Tous les champs continus en (x, z), évalués exactement.
    pub fn sample(&self, x: f64, z: f64) -> MacroSample {
        let (px, pz) = self.warped(x, z);
        let preset = self.cfg.preset;

        let o = self.ocean(x, z);

        // C : continentalité, biais d'océanité du préréglage.
        let c_noise = spread(self.c.fbm(px / 2600.0, pz / 2600.0, 5));
        let c_biased = clamp01(c_noise + Self::continentality_bias(x));
        let c = lerp(
            c_biased,
            Self::GULF_TARGET_C,
            Self::GULF_PULL * Self::gulf_weight(x),
        );

        // T₀ : nord froid, sud doux.
        let t_noise = spread(self.t0.fbm(px / 6000.0, pz / 6000.0, 2));
        let t0 =
            clamp01(0.65 * Self::warmth_bias(z) + 0.35 * t_noise + preset.temperature_offset());

        // Hm : humidité, plus forte côté océan.
        let hm = clamp01(spread(self.hm.fbm(px / 1900.0, pz / 1900.0, 4)) + 0.15 * o);

        // U : surrection, renforcée près de la façade atlantique.
        let u_raw = smooth(0.50, 0.78, spread(self.u.fbm(px / 3200.0, pz / 3200.0, 3)));
        let u = clamp01(u_raw * (0.6 + 0.6 * o) + preset.uplift_bonus());

        // E : érosion (1 = plat).
        let e = spread(self.e.fbm(px / 900.0, pz / 900.0, 4));

        // Lk : pays des mille lacs.
        let lk = spread(self.lk.fbm(x / 2200.0, z / 2200.0, 2));

        // Gt : géothermie, seulement au large (C bas).
        let gt_raw = smooth(
            0.90,
            0.96,
            spread(self.gt.fbm(x / 5000.0 + 137.0, z / 5000.0 + 61.0, 2)),
        );
        let gt = gt_raw * smooth(0.50, 0.42, c);

        // Vr : variante locale.
        let vr = spread(self.vr.fbm(x / 500.0, z / 500.0, 3));

        MacroSample {
            c,
            o,
            t0,
            hm,
            u,
            e,
            lk,
            gt,
            vr,
            theta: self.flow_direction(x, z),
        }
    }

    /// [`MacroFields::sample`] pour une colonne de blocs.
    pub fn sample_column(&self, wx: i32, wz: i32) -> MacroSample {
        self.sample(f64::from(wx), f64::from(wz))
    }
}

/// Champs d'un chunk : treillis de 5 × 5 nœuds espacés de 4 blocs, interpolés bilinéairement.
///
/// Les nœuds sont ancrés sur des multiples de 4 en coordonnées monde : deux chunks voisins
/// calculent exactement la même valeur sur leur arête commune, et le résultat ne dépend pas de
/// l'ordre de génération. Coût : 25 évaluations au lieu de 256 par chunk.
#[derive(Debug, Clone)]
pub struct ChunkFields {
    origin_x: i32,
    origin_z: i32,
    nodes: [MacroSample; 25],
}

impl ChunkFields {
    /// Calcule le treillis du chunk `(cx, cz)` (chunks de 16 × 16 colonnes).
    pub fn new(fields: &MacroFields, cx: i32, cz: i32) -> Self {
        let (origin_x, origin_z) = (cx * 16, cz * 16);
        let first = fields.sample_column(origin_x, origin_z);
        let mut nodes = [first; 25];
        for nz in 0..5 {
            for nx in 0..5 {
                if nx == 0 && nz == 0 {
                    continue;
                }
                nodes[nz * 5 + nx] = fields.sample_column(
                    origin_x + nx as i32 * LATTICE_STEP,
                    origin_z + nz as i32 * LATTICE_STEP,
                );
            }
        }
        Self {
            origin_x,
            origin_z,
            nodes,
        }
    }

    /// Champs interpolés à la colonne locale `(lx, lz)` du chunk, `0 ≤ lx, lz ≤ 15`.
    pub fn at(&self, lx: i32, lz: i32) -> MacroSample {
        debug_assert!((0..16).contains(&lx) && (0..16).contains(&lz));
        let (ix, iz) = ((lx / LATTICE_STEP) as usize, (lz / LATTICE_STEP) as usize);
        let tx = f64::from(lx % LATTICE_STEP) / f64::from(LATTICE_STEP);
        let tz = f64::from(lz % LATTICE_STEP) / f64::from(LATTICE_STEP);
        MacroSample::bilinear(
            &self.nodes[iz * 5 + ix],
            &self.nodes[iz * 5 + ix + 1],
            &self.nodes[(iz + 1) * 5 + ix],
            &self.nodes[(iz + 1) * 5 + ix + 1],
            tx,
            tz,
        )
    }

    /// Coin (x, z) du chunk, en blocs.
    pub fn origin(&self) -> (i32, i32) {
        (self.origin_x, self.origin_z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::norrcraft::config::NorrPreset;

    fn fields(seed: i64) -> MacroFields {
        MacroFields::new(seed, NorrConfig::default())
    }

    fn all_values(s: &MacroSample) -> [f64; 9] {
        [s.c, s.o, s.t0, s.hm, s.u, s.e, s.lk, s.gt, s.vr]
    }

    #[test]
    fn fields_are_in_unit_range() {
        for seed in [1i64, 12345, -777] {
            for preset in NorrPreset::ALL {
                let f = MacroFields::new(seed, NorrConfig::with_preset(preset));
                for i in -30..30 {
                    for j in -30..30 {
                        let s = f.sample(f64::from(i) * 531.0, f64::from(j) * 487.0);
                        for v in all_values(&s) {
                            assert!((0.0..=1.0).contains(&v), "{preset:?} {s:?}");
                        }
                        let n = (s.theta[0] * s.theta[0] + s.theta[1] * s.theta[1]).sqrt();
                        assert!((n - 1.0).abs() < 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn sampling_is_pure_and_order_independent() {
        let f = fields(42);
        let g = fields(42);
        let pts = [(0.0, 0.0), (-8123.4, 77.7), (12000.0, -9000.0), (3.0, 4.0)];
        let a: Vec<_> = pts.iter().map(|&(x, z)| f.sample(x, z)).collect();
        let b: Vec<_> = pts.iter().rev().map(|&(x, z)| g.sample(x, z)).collect();
        for (i, s) in a.iter().enumerate() {
            let t = &b[pts.len() - 1 - i];
            assert_eq!(
                all_values(s).map(f64::to_bits),
                all_values(t).map(f64::to_bits)
            );
            assert_eq!(s.theta.map(f64::to_bits), t.theta.map(f64::to_bits));
        }
        assert_ne!(
            f.sample(100.0, 100.0).c.to_bits(),
            fields(43).sample(100.0, 100.0).c.to_bits(),
            "la graine change le monde"
        );
    }

    /// Valeurs de référence figées : toute modification d'une formule ou d'une constante les
    /// change. À mettre à jour consciemment (et à refaire sous Linux et Windows).
    #[test]
    fn reference_values_seed_12345() {
        let f = fields(12345);
        let s = f.sample(0.0, 0.0);
        let t = f.sample(-6000.0, -4000.0);
        assert_eq!(REF0, [s.c, s.o, s.t0, s.hm, s.u, s.e, s.lk, s.gt, s.vr]);
        assert_eq!(REF1, [t.c, t.o, t.t0, t.hm, t.u, t.e, t.lk, t.gt, t.vr]);
    }
    const REF0: [f64; 9] = [
        0.6728762556226175,
        0.5637335299211184,
        0.6014374106307567,
        0.5120814271552735,
        0.0,
        0.7083191306929205,
        0.7373448618561858,
        0.0,
        0.2619092120579295,
    ];
    const REF1: [f64; 9] = [
        0.6810208997292713,
        0.5401781450549421,
        0.36323408428150755,
        0.6743030302933914,
        0.16663933330056907,
        0.5483220246551813,
        0.21416649516600883,
        0.0,
        0.8423678364200622,
    ];

    #[test]
    fn fields_are_continuous() {
        // Un pas d'un bloc ne doit jamais faire sauter un champ (hors θ aux frontières, lissé aussi).
        let f = fields(7);
        let mut worst = [0.0f64; 9];
        let mut prev = f.sample(-3000.0, 1000.0);
        for k in 1..6000 {
            let s = f.sample(-3000.0 + f64::from(k), 1000.0 + f64::from(k) * 0.3);
            for (w, (a, b)) in worst
                .iter_mut()
                .zip(all_values(&s).into_iter().zip(all_values(&prev)))
            {
                *w = w.max((a - b).abs());
            }
            prev = s;
        }
        for (name, w) in ["c", "o", "t0", "hm", "u", "e", "lk", "gt", "vr"]
            .iter()
            .zip(worst)
        {
            assert!(w < 0.03, "champ {name} : saut de {w} en un bloc");
        }
    }

    #[test]
    fn norrland_geography_is_oriented() {
        // Moyennes sur de grandes surfaces : ouest = océan profond, est = golfe peu profond,
        // nord = froid, sud = doux.
        let f = fields(2024);
        let mean = |cx: f64, cz: f64, field: fn(&MacroSample) -> f64| {
            let (mut sum, mut n) = (0.0, 0.0);
            for i in -10..10 {
                for j in -10..10 {
                    let s = f.sample(cx + f64::from(i) * 400.0, cz + f64::from(j) * 400.0);
                    sum += field(&s);
                    n += 1.0;
                }
            }
            sum / n
        };
        let c_west = mean(-14000.0, 0.0, |s| s.c);
        let c_mid = mean(0.0, 0.0, |s| s.c);
        let c_east = mean(14000.0, 0.0, |s| s.c);
        assert!(c_west < 0.2, "ouest : {c_west}");
        assert!(c_east > 0.25 && c_east < COAST_C, "est : {c_east}");
        assert!(c_mid > c_east && c_mid > c_west, "centre : {c_mid}");
        assert!(mean(-14000.0, 0.0, |s| s.o) > mean(14000.0, 0.0, |s| s.o) + 0.3);
        assert!(mean(0.0, 14000.0, |s| s.t0) > mean(0.0, -14000.0, |s| s.t0) + 0.35);
    }

    #[test]
    fn presets_modify_the_fields_as_documented() {
        let base = MacroFields::new(9, NorrConfig::with_preset(NorrPreset::Norrland));
        let hog = MacroFields::new(9, NorrConfig::with_preset(NorrPreset::Hogfjall));
        let ska = MacroFields::new(9, NorrConfig::with_preset(NorrPreset::Skargard));
        for i in -10..10 {
            for j in -10..10 {
                let (x, z) = (f64::from(i) * 777.0, f64::from(j) * 631.0);
                let (a, h, s) = (base.sample(x, z), hog.sample(x, z), ska.sample(x, z));
                assert!(h.u >= a.u && h.u <= (a.u + 0.25 + 1e-12).min(1.0));
                assert!((h.t0 - (a.t0 - 0.15).max(0.0)).abs() < 1e-12);
                assert_eq!(
                    s.c.to_bits(),
                    a.c.to_bits(),
                    "Skärgård ne change que la hauteur"
                );
                assert_eq!(h.c.to_bits(), a.c.to_bits());
            }
        }
    }

    #[test]
    fn spread_makes_fields_use_their_whole_range() {
        // Sans biais (champs purement bruités) les déciles sont à peu près équilibrés.
        let f = fields(31337);
        let mut e = [0u32; 10];
        let mut lk = [0u32; 10];
        let mut n = 0u32;
        for i in -60..60 {
            for j in -60..60 {
                let s = f.sample(f64::from(i) * 300.0, f64::from(j) * 300.0);
                e[((s.e * 10.0) as usize).min(9)] += 1;
                lk[((s.lk * 10.0) as usize).min(9)] += 1;
                n += 1;
            }
        }
        // Déciles centraux équilibrés ; les queues existent (valeurs sous 0,1 et au-dessus de 0,9).
        for d in e[2..8].iter().chain(lk[2..8].iter()) {
            let share = f64::from(*d) / f64::from(n);
            assert!(share > 0.05 && share < 0.20, "{e:?} {lk:?}");
        }
        assert!(
            e[0] > 0 && e[9] > 0 && lk[0] > 0 && lk[9] > 0,
            "{e:?} {lk:?}"
        );
    }

    #[test]
    fn geothermal_provinces_exist_only_offshore() {
        let f = fields(5);
        let (mut offshore, mut hot) = (0u32, 0u32);
        for i in -150..150 {
            for j in -150..150 {
                let s = f.sample(f64::from(i) * 250.0, f64::from(j) * 250.0);
                if s.gt > 0.0 {
                    assert!(s.c < 0.5, "Gt > 0 sur terre : {s:?}");
                }
                if s.gt > 0.5 {
                    hot += 1;
                }
                offshore += u32::from(s.c < 0.465);
            }
        }
        assert!(offshore > 0);
        assert!(hot < 90_000 / 10, "la province de lave reste rare : {hot}");
    }

    #[test]
    fn province_shares_are_in_the_expected_range() {
        let f = fields(2025);
        let mut count = [0u32; 8];
        let mut total = 0u32;
        for i in -200..200 {
            for j in -200..200 {
                let p = f.province(f64::from(i) * 400.0, f64::from(j) * 400.0);
                count[p as usize] += 1;
                total += 1;
            }
        }
        for p in Province::ALL {
            let share = f64::from(count[p as usize]) * 100.0 / f64::from(total);
            let target = p.share();
            // Porphyre (1 %) : très peu de cellules, tolérance large.
            let tol = if target < 2.0 {
                1.5
            } else {
                0.5 * target + 1.0
            };
            assert!(
                (share - target).abs() < tol,
                "{p:?} : {share:.1} % pour {target} %"
            );
        }
    }

    #[test]
    fn flow_direction_is_unit_and_continuous() {
        let f = fields(8);
        let mut prev = f.flow_direction(-9000.0, 123.0);
        for k in 1..20_000 {
            let v = f.flow_direction(-9000.0 + f64::from(k), 123.0);
            assert!(((v[0] * v[0] + v[1] * v[1]).sqrt() - 1.0).abs() < 1e-9);
            let (dx, dz) = (v[0] - prev[0], v[1] - prev[1]);
            let jump = (dx * dx + dz * dz).sqrt();
            // Le seul endroit où θ tourne vite : là où deux flux opposés se compensent presque.
            assert!(jump < 0.15, "θ saute de {jump} à k = {k}");
            prev = v;
        }
    }

    #[test]
    fn lattice_matches_exact_sampling_closely() {
        let f = fields(777);
        let mut worst = [0.0f64; 9];
        let mut sum_err = 0.0;
        let mut count = 0.0;
        for cx in -12..12 {
            for cz in -12..12 {
                let cf = ChunkFields::new(&f, cx * 7, cz * 5);
                let (ox, oz) = cf.origin();
                for lz in 0..16 {
                    for lx in 0..16 {
                        let exact = f.sample_column(ox + lx, oz + lz);
                        let approx = cf.at(lx, lz);
                        for (w, (a, b)) in worst
                            .iter_mut()
                            .zip(all_values(&approx).into_iter().zip(all_values(&exact)))
                        {
                            let d = (a - b).abs();
                            *w = w.max(d);
                            sum_err += d;
                            count += 1.0;
                        }
                    }
                }
            }
        }
        for (name, w) in ["c", "o", "t0", "hm", "u", "e", "lk", "gt", "vr"]
            .iter()
            .zip(worst)
        {
            assert!(w < 0.05, "treillis : champ {name}, erreur max {w}");
        }
        assert!(
            sum_err / count < 0.002,
            "erreur moyenne {}",
            sum_err / count
        );
    }

    #[test]
    fn lattice_is_exact_on_nodes_and_shared_between_chunks() {
        let f = fields(99);
        let a = ChunkFields::new(&f, 3, -4);
        let b = ChunkFields::new(&f, 4, -4);
        // Colonne x = 16 du monde : nœud commun (lx = 16 de a n'existe pas ; lx = 0 de b), mais
        // les colonnes 12 de a et 0 de b encadrent le même segment de treillis.
        let exact = f.sample_column(a.origin().0 + 12, a.origin().1 + 8);
        assert_eq!(
            a.at(12, 8).c.to_bits(),
            exact.c.to_bits(),
            "nœud : valeur exacte"
        );
        let exact_b = f.sample_column(b.origin().0, b.origin().1 + 8);
        assert_eq!(b.at(0, 8).u.to_bits(), exact_b.u.to_bits());
        // Le treillis interpolé de a à lx = 15 tend vers le nœud de b à lx = 0.
        let near = a.at(15, 8);
        assert!((near.c - b.at(0, 8).c).abs() < 0.05);
    }
}
