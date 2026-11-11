//! Fonction de hauteur Norrcraft (session 1.2 ; GDD doc 02 §5.1 à §5.4).
//!
//! `h(x, z)` est une composition, dans l'ordre du doc :
//!
//! 1. **socle** `B(C)` : spline lissée de la continentalité (§5.1) ;
//! 2. **collines et détail** `Hc + Dt` (§5.2) ;
//! 3. **chaîne des Scandes** `Mt` (crêtes déformées), **plateaux** (vidda) en paliers, **plafond
//!    doux** (§5.3) ;
//! 4. **vallées glaciaires** en U et **fjords**, plus des **vallées suspendues** (§5.4) ;
//! 5. **formes de relief** de la session 1.3 (§5.5 à §5.8 : archipel, côtes, eskers, tourbières,
//!    palses, champ de lave, glacier), voir [`super::landforms`].
//!
//! Chaque étape est continue (les paliers du doc, écrits avec `plancher`, sont adoucis par
//! [`soft_floor`]) : seules les parois des auges glaciaires sont des falaises franches, et elles
//! sont déclarées ([`Relief::wall`]). Les champs lents (C, U, E) arrivent déjà évalués (treillis
//! 4 × 4 du chunk ou de la colonne) ; tout le reste est calculé à la colonne.

use super::config::NorrConfig;
use super::fields::{MacroFields, MacroSample};
use super::landforms::{BaseRelief, Forms, Landforms};
use super::math::{bump, lerp, smooth, soft_cap, spread, GradField};

/// Constantes de semence des bruits du relief (`graine XOR constante`). Figées : en changer une
/// change tous les mondes (et les empreintes de `tests/norrcraft_relief.rs`).
mod seeds {
    pub const HILLS: u64 = 0x4E52_5743_0000_0101;
    pub const DETAIL: u64 = 0x4E52_5743_0000_0102;
    pub const CHAIN_WARP: u64 = 0x4E52_5743_0000_0103;
    pub const CHAIN: u64 = 0x4E52_5743_0000_0104;
    pub const PLATEAU: u64 = 0x4E52_5743_0000_0105;
    pub const PLATEAU_LEVEL: u64 = 0x4E52_5743_0000_0106;
    pub const VALLEY_WARP: u64 = 0x4E52_5743_0000_0107;
    pub const VALLEY: u64 = 0x4E52_5743_0000_0108;
    pub const HANGING: u64 = 0x4E52_5743_0000_0109;
}

/// Nœuds `(C, B(C) − S)` du socle continental (doc 02 §5.1).
pub const SOCLE_KNOTS: [(f64, f64); 7] = [
    (0.00, -28.0),
    (0.30, -13.0),
    (0.42, -4.0),
    (0.47, 1.0),
    (0.55, 6.0),
    (0.75, 13.0),
    (1.00, 20.0),
];

/// Début du plafond doux, au-dessus de S (h₀ = S + 62).
pub const CAP_START: f64 = 62.0;
/// Marge du plafond doux (asymptote à S + 62 + 22 = S + 84).
pub const CAP_MARGIN: f64 = 22.0;
/// Les fonds de vallées suspendues sont 18 blocs au-dessus de la vallée principale.
pub const HANGING_RISE: f64 = 18.0;

/// Socle `B(C) − S` : spline linéaire par morceaux de C, lissée par `sm` entre les nœuds
/// (dérivée nulle aux nœuds, donc continue et sans angle).
pub fn socle(c: f64) -> f64 {
    let k = &SOCLE_KNOTS;
    if c <= k[0].0 {
        return k[0].1;
    }
    for w in k.windows(2) {
        if c <= w[1].0 {
            return lerp(w[0].1, w[1].1, smooth(w[0].0, w[1].0, c));
        }
    }
    k[k.len() - 1].1
}

/// Escalier adouci : `plancher(v)` dont chaque marche monte en douceur sur la dernière fraction
/// `w` de l'intervalle (continu, plat sur `1 − w` de chaque marche).
#[inline]
pub fn soft_floor(v: f64, w: f64) -> f64 {
    let k = v.floor();
    k + smooth(1.0 - w, 1.0, v - k)
}

/// Profil en travers d'une auge glaciaire : `u(t) = 1 − t⁴` (fond plat, parois raides).
#[inline]
fn u_profile(t: f64) -> f64 {
    let t2 = t * t;
    1.0 - t2 * t2
}

/// Décomposition du relief d'une colonne : l'altitude finale et les termes utiles aux étapes
/// suivantes (biomes, hydrographie, décors).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Relief {
    /// Altitude finale du sol (en blocs, flottante), formes de la 1.3 comprises. Le premier bloc
    /// au-dessus du sol est `plancher(h)` (air, eau, ou glace d'un glacier : voir [`Forms::ice`]).
    pub h: f64,
    /// Socle `B(C)` (préréglage compris).
    pub socle: f64,
    /// Poids de terre `sm(0,45 ; 0,52 ; C)`.
    pub land: f64,
    /// Apport de la chaîne `Mt`.
    pub chain: f64,
    /// Altitude avant les vallées (`h₃` après plafond doux).
    pub before_valleys: f64,
    /// Poids du plateau `P · bump(h₂ ; ℓ, 14)` (1 = plateau tiré vers son palier).
    pub plateau: f64,
    /// Palier de plateau ℓ (S + 44, S + 56 ou S + 68, adouci).
    pub plateau_level: f64,
    /// `g · u(t)` de la vallée principale (1 = fond d'auge).
    pub valley: f64,
    /// Altitude visée du fond de la vallée principale, `f`.
    pub valley_floor: f64,
    /// `g · u(t)` de la vallée suspendue.
    pub hanging: f64,
    /// Étiquette **Fjord** (doc 02 §5.4) : `g · u(t) > 0,5`, `f < S` et `C < 0,56`.
    pub fjord: bool,
    /// Formes de relief de la session 1.3 (archipel, côtes, eskers, tourbières, glace, lave).
    pub forms: Forms,
}

impl Relief {
    /// Premier `y` libre de la colonne (surface, glace comprise), borné à `[1, hauteur − 1]`.
    pub fn top(&self, world_height: i32) -> i32 {
        ((self.h + self.forms.ice).floor() as i32).clamp(1, world_height - 1)
    }

    /// Premier `y` au-dessus du sol (roche et couverture meuble, sans la glace), borné comme
    /// [`Relief::top`].
    pub fn ground_top(&self, world_height: i32) -> i32 {
        (self.h.floor() as i32).clamp(1, world_height - 1)
    }

    /// Falaise déclarée : flanc d'une vallée glaciaire (ni fond, ni hors de l'auge), klint,
    /// crevasse ou fissure volcanique ; les marches de plus de 3 blocs y sont permises (doc 02 §13).
    pub fn wall(&self) -> bool {
        let side = |w: f64| w > 0.01 && w < 0.99;
        side(self.valley) || side(self.hanging) || self.forms.cliff
    }
}

/// Fonction de hauteur d'un monde (graine + configuration). Cloner est peu coûteux.
#[derive(Debug, Clone)]
pub struct HeightFields {
    cfg: NorrConfig,
    macro_fields: MacroFields,
    hills: GradField,
    detail: GradField,
    chain_warp: GradField,
    chain: GradField,
    plateau: GradField,
    plateau_level: GradField,
    valley_warp: GradField,
    valley: GradField,
    hanging: GradField,
    landforms: Landforms,
}

impl HeightFields {
    pub fn new(seed: i64, cfg: NorrConfig) -> Self {
        Self {
            cfg,
            macro_fields: MacroFields::new(seed, cfg),
            hills: GradField::new(seed, seeds::HILLS),
            detail: GradField::new(seed, seeds::DETAIL),
            chain_warp: GradField::new(seed, seeds::CHAIN_WARP),
            chain: GradField::new(seed, seeds::CHAIN),
            plateau: GradField::new(seed, seeds::PLATEAU),
            plateau_level: GradField::new(seed, seeds::PLATEAU_LEVEL),
            valley_warp: GradField::new(seed, seeds::VALLEY_WARP),
            valley: GradField::new(seed, seeds::VALLEY),
            hanging: GradField::new(seed, seeds::HANGING),
            landforms: Landforms::new(seed),
        }
    }

    pub fn config(&self) -> &NorrConfig {
        &self.cfg
    }

    /// Champs macroscopiques du même monde.
    pub fn macro_fields(&self) -> &MacroFields {
        &self.macro_fields
    }

    /// Relief de la colonne `(wx, wz)`, champs lents lus par le treillis 4 × 4 : identique au
    /// relief calculé pendant la génération du chunk qui contient la colonne.
    pub fn column(&self, wx: i32, wz: i32) -> Relief {
        let m = self.macro_fields.sample_lattice(wx, wz);
        self.relief(f64::from(wx), f64::from(wz), &m)
    }

    /// Relief au point `(x, z)` (en blocs), champs lents évalués exactement (cartes).
    pub fn relief_exact(&self, x: f64, z: f64) -> Relief {
        let m = self.macro_fields.sample(x, z);
        self.relief(x, z, &m)
    }

    /// Relief au point `(x, z)` à partir de ses champs macroscopiques `m`.
    pub fn relief(&self, x: f64, z: f64, m: &MacroSample) -> Relief {
        let s = f64::from(self.cfg.sea_level);
        let (c, u, e) = (m.c, m.u, m.e);

        // §5.1 Socle continental.
        let b = s + socle(c) + self.cfg.preset.socle_offset();

        // §5.2 Collines et détail.
        let land = smooth(0.45, 0.52, c);
        let hc =
            land * (6.0 + 10.0 * (1.0 - e)) * (2.0 * self.hills.fbm(x / 260.0, z / 260.0, 4) - 1.0);
        let dt = 1.6 * (2.0 * self.detail.fbm(x / 48.0, z / 48.0, 3) - 1.0)
            + 0.6 * (2.0 * self.detail.n(x / 11.0 + 101.3, z / 11.0 + 57.9) - 1.0);
        let h1 = b + hc + dt;

        // §5.3 Chaîne des Scandes.
        let (pmx, pmz) = self.chain_warp.warp(x, z, 90.0, 420.0);
        let rg = self.chain.ridged(pmx / 1500.0, pmz / 1500.0, 5);
        let chain = u * land * (22.0 + 50.0 * rg) * (0.35 + 0.65 * (1.0 - e));

        // Plateaux (vidda) : paliers S + 44, S + 56, S + 68 (adoucis : voir `soft_floor`).
        let p_mask = smooth(
            0.55,
            0.70,
            spread(self.plateau.fbm(x / 700.0, z / 700.0, 2)),
        ) * u;
        let level_n = spread(self.plateau_level.fbm(x / 1600.0, z / 1600.0, 2));
        let level = s + 44.0 + 12.0 * soft_floor((3.0 * level_n).min(2.999), 0.3);
        // Chaîne, plateaux et plafond doux appliqués à `h₁` : `(h₃, poids du plateau)`. Servent
        // deux fois : au relief et au relief filtré `Hl` (sans les collines ni le détail).
        let raise = |h1: f64| {
            let h2 = h1 + chain;
            let plateau = p_mask * bump(h2, level, 14.0);
            let h3 = h2 + plateau * 0.75 * (level - h2);
            (soft_cap(h3, s + CAP_START, CAP_MARGIN), plateau)
        };
        let (h3, plateau) = raise(h1);

        // §5.4 Vallées glaciaires et fjords.
        let (vx, vz) = self.valley_warp.warp(x, z, 160.0, 520.0);
        let half_width = 0.050 + 0.035 * (1.0 - c);
        let t = (self.valley.zero_line(vx / 1100.0, vz / 1100.0, 3) / half_width).min(1.0);
        let inland = s + 2.0 + 6.0 * soft_floor(5.0 * smooth(0.56, 0.82, c), 0.3);
        let fjord_floor = s - 6.0 - 24.0 * smooth(0.56, 0.47, c) + 15.0 * bump(c, 0.445, 0.02);
        let floor = lerp(fjord_floor, inland, smooth(0.54, 0.60, c));
        // Seulement dans la chaîne ; s'éteint au large (au-delà du seuil) pour que les fjords
        // débouchent sur le plateau continental au lieu d'y creuser une fosse.
        let g = smooth(0.25, 0.50, u) * smooth(0.40, 0.44, c);

        // Vallées suspendues : auges plus étroites, fond 18 blocs au-dessus de la principale.
        let t2 = (self.hanging.zero_line(vx / 450.0, vz / 450.0, 3) / half_width).min(1.0);
        let hanging = g * u_profile(t2);
        let valley = g * u_profile(t);
        let carve = |h3: f64| {
            let h3b = h3 - hanging * (h3 - (floor + HANGING_RISE)).max(0.0);
            h3b - valley * (h3b - floor).max(0.0)
        };
        let h4 = carve(h3);
        let filtered = carve(raise(b).0);

        // §5.5 à §5.8 Formes de relief (session 1.3).
        let base = BaseRelief {
            h: h4,
            filtered,
            land,
            valley,
            before_valleys: h3,
        };
        let (h, forms) = self
            .landforms
            .apply(x, z, m, &self.macro_fields, &self.cfg, base);

        Relief {
            h,
            socle: b,
            land,
            chain,
            before_valleys: h3,
            plateau,
            plateau_level: level,
            valley,
            valley_floor: floor,
            hanging,
            fjord: valley > 0.5 && floor < s && c < 0.56,
            forms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::norrcraft::NorrPreset;

    #[test]
    fn socle_hits_the_knots_and_is_monotonic() {
        for (c, v) in SOCLE_KNOTS {
            assert_eq!(socle(c), v);
        }
        let mut prev = socle(0.0);
        for i in 1..=1000 {
            let v = socle(f64::from(i) / 1000.0);
            assert!(v >= prev);
            prev = v;
        }
        // Le trait de côte (B = S) tombe sur le plateau continental, près de C ≈ 0,46.
        let coast = (0..1000)
            .map(|i| f64::from(i) / 1000.0)
            .find(|&c| socle(c) >= 0.0)
            .unwrap();
        assert!((0.44..0.47).contains(&coast), "côte à C = {coast}");
    }

    #[test]
    fn soft_floor_is_a_continuous_staircase() {
        assert_eq!(soft_floor(0.2, 0.3), 0.0);
        assert_eq!(soft_floor(1.5, 0.3), 1.0);
        assert_eq!(soft_floor(2.0, 0.3), 2.0);
        let mut prev = soft_floor(0.0, 0.3);
        for i in 1..=3000 {
            let v = soft_floor(f64::from(i) / 1000.0, 0.3);
            assert!(v >= prev && v - prev < 0.01);
            prev = v;
        }
    }

    #[test]
    fn heights_stay_inside_the_world() {
        for preset in NorrPreset::ALL {
            let f = HeightFields::new(777, NorrConfig::with_preset(preset));
            let s = f64::from(f.config().sea_level);
            for j in -40..40 {
                for i in -40..40 {
                    let r = f.relief_exact(f64::from(i * 97), f64::from(j * 89));
                    assert!(r.h > 4.0, "{preset:?} : {}", r.h);
                    assert!(r.h < s + CAP_START + CAP_MARGIN, "{preset:?} : {}", r.h);
                }
            }
        }
    }

    #[test]
    fn column_matches_exact_relief_closely() {
        let f = HeightFields::new(12345, NorrConfig::default());
        for (x, z) in [(0, 0), (-731, 402), (5003, -2999), (-6100, 7777)] {
            let a = f.column(x, z).h;
            let b = f.relief_exact(f64::from(x), f64::from(z)).h;
            assert!((a - b).abs() < 1.0, "({x}, {z}) : treillis {a}, exact {b}");
        }
    }
}
