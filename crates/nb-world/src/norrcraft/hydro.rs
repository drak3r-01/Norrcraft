//! Hydrographie Norrcraft (session 1.4 ; GDD doc 02 §6).
//!
//! L'eau du moteur est statique : toute nappe doit être **horizontale et étanche** par
//! construction. Le module part du relief de [`HeightFields`] et produit, pour chaque colonne, une
//! [`WaterColumn`] : le premier `y` non solide (`top`), le premier `y` d'air au-dessus de l'eau
//! (`level`) et la nature de l'eau ([`WaterKind`]).
//!
//! Dans l'ordre :
//!
//! 1. **niveau en escalier** `W = S + 6 · plancher(max(0, Hl − S − 2) / 6)` (§6.1), `Hl` étant le
//!    relief filtré (sans collines ni détail, [`filtered_height`]) ; tout creux du relief sous `W`
//!    se remplit à `W` (la nappe) : c'est la mer sur la côte, des étangs et des lacs de creux
//!    dans les terres. Juste au-dessus de chaque marche, les creux sont comblés jusqu'à `W` : deux
//!    nappes de niveaux différents ne se touchent jamais en plein champ ;
//! 2. **lacs de vallée** (fingersjöar) : tronçons d'auge intérieure surcreusés de 8 blocs ;
//! 3. **lacs** sur un Worley de pas 180 (§6.3) : niveau constant, cuvette, contour bruité (îles),
//!    berges talutées et **margelle** ;
//! 4. **rivières** (§6.2) : lignes de zéro d'un bruit lent, lit en V adouci calé sur `W`, barre
//!    rocheuse au-dessus de chaque marche, bassin en aval ; dans la chaîne, la rivière suit le fond
//!    des auges glaciaires ;
//! 5. **scellement** ([`seal`]) : une colonne voisine d'une eau plus haute qu'elle est montée
//!    jusqu'au niveau de cette eau (barrage d'un bloc). Une seule passe sur les 4 voisines suffit :
//!    aucun bloc d'eau n'a jamais d'air pour voisin latéral, quel que soit le relief.
//!
//! Tout est fonction pure de (graine, configuration, colonne et ses 4 voisines) : un chunk
//! calcule sa bordure, une colonne isolée ses 4 voisines, et les deux coïncident.

use super::config::NorrConfig;
use super::fields::MacroSample;
use super::height::{HeightFields, Relief};
use super::math::{smooth, spread, GradField, Worley};

/// Constantes de semence de l'hydrographie (`graine XOR constante`). Figées.
mod seeds {
    pub const RIVER_WARP: u64 = 0x4E52_5743_0000_0201;
    pub const RIVER: u64 = 0x4E52_5743_0000_0202;
    pub const LAKES: u64 = 0x4E52_5743_0000_0203;
    pub const LAKE_SHAPE: u64 = 0x4E52_5743_0000_0204;
    pub const FINGER: u64 = 0x4E52_5743_0000_0205;
}

/// Hauteur d'une marche de l'escalier d'eau (doc 02 §6.1).
pub const STAIR_STEP: i32 = 6;
/// Pas du Worley des lacs (doc 02 §6.3).
pub const LAKE_PITCH: f64 = 180.0;
/// Rayon de base des lacs, de 14 à 60 blocs (le contour bruité le module de 0,6 à 1,4 ; ×1,6 le long de θ).
pub const LAKE_RADIUS: (f64, f64) = (14.0, 60.0);
/// Allongement des lacs le long du sens d'écoulement des glaces θ (lacs de surcreusement).
pub const LAKE_STRETCH: f64 = 1.6;
/// Portée maximale de l'influence d'un lac au-delà de son contour (berges talutées, margelle).
pub const LAKE_BANK_REACH: f64 = 30.0;
/// Largeur de la margelle : sur cette bande au-delà du contour, le sol est au moins au niveau.
pub const RIM_WIDTH: f64 = 4.0;
/// Pente des berges taillées par les lacs et les rivières (blocs de hauteur par bloc).
const LAKE_BANK_SLOPE: f64 = 0.6;
const RIVER_BANK_SLOPE: f64 = 0.8;
/// Fraction de marche, juste au-dessus d'une marche de `W`, où les creux sont comblés et où le lit
/// des rivières remonte en barre rocheuse.
const STEP_SILL: f64 = 0.15;
/// Fraction de marche, juste sous la marche suivante, où le lit est surcreusé (bassin de chute).
const PLUNGE_FROM: f64 = 0.85;
/// Demi-largeur approximative des auges glaciaires, en blocs par unité de `t` (§5.4).
const TROUGH_BLOCKS: f64 = 35.0;

/// Nature de l'eau d'une colonne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WaterKind {
    /// Colonne sèche.
    None,
    /// Mer (au niveau S, reliée au large : continentalité basse, fjord ou relief filtré sous S).
    Sea,
    /// Lac (Worley de §6.3), lac de vallée, étang de creux ou ancienne « mer intérieure ».
    Lake,
    /// Lit d'une rivière.
    River,
}

impl WaterKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "sèche",
            Self::Sea => "mer",
            Self::Lake => "lac",
            Self::River => "rivière",
        }
    }
}

/// Façade maritime (doc 02 §6.4) : sert aux biomes marins (2.1) et à la banquise (doc 04 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeaFacade {
    /// O > 0,5 : eau salée, laminaires, jamais gelée.
    Atlantic,
    /// O ≤ 0,5 : eau saumâtre (même bloc d'eau), banquise saisonnière.
    Baltic,
    /// T₀ < 0,12 : banquise permanente (blocs de glace en 2.1, faute de bloc dans la palette).
    Arctic,
}

impl SeaFacade {
    pub fn of(m: &MacroSample) -> Self {
        if m.t0 < 0.12 {
            Self::Arctic
        } else if m.o > 0.5 {
            Self::Atlantic
        } else {
            Self::Baltic
        }
    }
}

/// Eau d'une colonne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterColumn {
    /// Premier `y` non solide (fond de l'eau, ou surface du sol si la colonne est sèche).
    pub top: i32,
    /// Premier `y` d'air au-dessus de l'eau. L'eau occupe `top..level` ; colonne sèche si
    /// `level <= top` (le niveau reste alors celui de la nappe locale, pour information).
    pub level: i32,
    pub kind: WaterKind,
    /// Niveau en escalier `W` de la colonne.
    pub stair: i32,
    /// Distance estimée à l'axe du lit actif le plus proche (rivière de plaine ou fond d'auge),
    /// en blocs (∞ hors réseau ou dans un lac) : sert aux cartes à grand pas.
    pub channel_distance: f64,
    /// Demi-largeur locale du lit des rivières, en blocs.
    pub river_width: f64,
    /// Marche déclarée (doc 02 §13 : « marches de W ») : colonne montée par le scellement
    /// (barrage, lèvre de chute) ou barre rocheuse comblant un creux juste au-dessus d'une marche.
    pub dam: bool,
}

impl WaterColumn {
    #[inline]
    pub fn is_wet(&self) -> bool {
        self.level > self.top
    }

    /// Altitude de la surface visible : le sol, ou la surface de l'eau.
    #[inline]
    pub fn surface(&self) -> i32 {
        self.top.max(self.level)
    }

    /// Niveau « efficace » vu par une voisine : jusqu'où la colonne est pleine (sol ou eau).
    #[inline]
    fn filled(&self) -> i32 {
        if self.is_wet() {
            self.level
        } else {
            self.top
        }
    }
}

/// Scellement (étanchéité, doc 02 §6.3 et §13) : la colonne `c` est montée jusqu'au niveau de
/// toute voisine mouillée dont l'eau dépasse ce que `c` remplit déjà. Appliqué sur les colonnes
/// **brutes** (avant scellement) : une passe garantit qu'aucun bloc d'eau n'a d'air pour voisin
/// latéral (une colonne montée est pleine jusqu'au plus haut niveau voisin ; une colonne mouillée
/// qui le reste a des voisines remplies au moins jusqu'à son niveau).
pub fn seal(c: WaterColumn, neighbours: [WaterColumn; 4], world_height: i32) -> WaterColumn {
    let own = c.filled();
    let mut top = c.top;
    for n in neighbours {
        if n.is_wet() && n.level > own {
            top = top.max(n.level);
        }
    }
    let top = top.min(world_height - 1);
    let mut out = c;
    if top > c.top {
        out.top = top;
        out.dam = true;
        if !out.is_wet() {
            out.kind = WaterKind::None;
        }
    }
    out
}

/// Relief filtré `Hl` (doc 02 §6.1) : le relief de la colonne sans les collines ni le détail
/// (socle, chaîne, plateaux, plafond doux, vallées). Depuis la 1.3, la fonction de hauteur le
/// calcule elle-même (même chaîne de calcul appliquée au socle seul) et le range dans
/// [`Forms::filtered`](super::landforms::Forms) : les tourbières s'y calent aussi.
pub fn filtered_height(r: &Relief, _sea: f64) -> f64 {
    r.forms.filtered
}

/// Niveau en escalier `W` et position dans la marche (dans [0, 1) ; 0 juste au-dessus d'une
/// marche, vers 1 juste sous la suivante).
pub fn stair_level(hl: f64, sea: i32) -> (i32, f64) {
    let v = (hl - f64::from(sea) - 2.0).max(0.0) / f64::from(STAIR_STEP);
    let k = v.floor();
    (sea + STAIR_STEP * k as i32, v - k)
}

/// Profil en travers d'un lit de rivière calé sur le niveau `wl` : V adouci de 2 à 5 blocs sous
/// l'eau sur la demi-largeur `w`, puis berge talutée.
#[inline]
fn channel(d: f64, w: f64, wl: f64) -> f64 {
    if d < w {
        let t = d / w;
        wl - 2.0 - 3.0 * (1.0 - t * t)
    } else {
        wl - 2.0 + (d - w) * RIVER_BANK_SLOPE
    }
}

/// Un lac du Worley de §6.3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lake {
    pub center: [f64; 2],
    /// Rayon de base (le contour bruité varie de 0,6 à 1,4 fois).
    pub radius: f64,
    /// Profondeur de la cuvette au centre.
    pub depth: f64,
    /// Premier `y` d'air au-dessus de l'eau du lac (constant sur tout le lac).
    pub level: i32,
    /// Grand axe (vecteur unitaire, sens d'écoulement des glaces au centre).
    pub axis: [f64; 2],
}

impl Lake {
    /// Distance (anisotrope, voir [`Lake::distance`]) au-delà de laquelle le lac n'a plus d'effet.
    /// En blocs, la portée ne dépasse jamais `1,6 × (1,4 × 60 + 30) = 182` < 198 : le voisinage
    /// 3 × 3 des cellules du Worley suffit.
    pub fn reach(&self) -> f64 {
        self.radius * 1.4 + LAKE_BANK_REACH
    }

    /// Distance anisotrope au centre : la composante le long du grand axe est divisée par
    /// [`LAKE_STRETCH`].
    pub fn distance(&self, x: f64, z: f64) -> f64 {
        let (dx, dz) = (x - self.center[0], z - self.center[1]);
        let along = (dx * self.axis[0] + dz * self.axis[1]) / LAKE_STRETCH;
        let across = dx * self.axis[1] - dz * self.axis[0];
        (along * along + across * across).sqrt()
    }
}

/// Lacs des cellules de Worley qui couvrent un rectangle de colonnes (avec la portée maximale
/// d'un lac). Calculés une fois par chunk, par requête isolée ou par carte.
#[derive(Debug, Clone)]
pub struct LakeSet {
    i0: i32,
    j0: i32,
    ni: i32,
    nj: i32,
    cells: Vec<Option<Lake>>,
}

impl LakeSet {
    /// Couvre les colonnes `x_min..=x_max` × `z_min..=z_max`.
    pub fn new(
        hydro: &Hydrology,
        heights: &HeightFields,
        x_min: i32,
        z_min: i32,
        x_max: i32,
        z_max: i32,
    ) -> Self {
        let cell = |v: i32| (f64::from(v) / LAKE_PITCH).floor() as i32;
        let (i0, j0) = (cell(x_min) - 1, cell(z_min) - 1);
        let (ni, nj) = (cell(x_max) + 2 - i0, cell(z_max) + 2 - j0);
        let mut cells = Vec::with_capacity((ni * nj) as usize);
        for j in 0..nj {
            for i in 0..ni {
                cells.push(hydro.lake_at_cell(heights, i0 + i, j0 + j));
            }
        }
        Self {
            i0,
            j0,
            ni,
            nj,
            cells,
        }
    }

    /// Lacs dont l'influence peut toucher la colonne `(x, z)` (les 3 × 3 cellules autour).
    fn near(&self, x: f64, z: f64) -> impl Iterator<Item = &Lake> {
        let ci = (x / LAKE_PITCH).floor() as i32 - self.i0;
        let cj = (z / LAKE_PITCH).floor() as i32 - self.j0;
        debug_assert!(
            ci >= 1 && cj >= 1 && ci + 1 < self.ni && cj + 1 < self.nj,
            "colonne hors des lacs"
        );
        (-1..=1)
            .flat_map(move |dj| (-1..=1).map(move |di| (ci + di, cj + dj)))
            .filter_map(move |(i, j)| self.cells[(j * self.ni + i) as usize].as_ref())
    }

    /// Tous les lacs présents (cartes, tests).
    pub fn lakes(&self) -> impl Iterator<Item = &Lake> {
        self.cells.iter().flatten()
    }
}

/// Bruits et règles de l'hydrographie d'un monde. Cloner est peu coûteux.
#[derive(Debug, Clone)]
pub struct Hydrology {
    sea: i32,
    world_height: i32,
    lakes: Worley,
    lake_shape: GradField,
    river_warp: GradField,
    river: GradField,
    finger: GradField,
}

impl Hydrology {
    pub fn new(seed: i64, cfg: NorrConfig) -> Self {
        Self {
            sea: cfg.sea_level,
            world_height: cfg.world_height.min(crate::chunk::CHUNK_SIZE_Y as i32),
            lakes: Worley::new(seed, seeds::LAKES, LAKE_PITCH),
            lake_shape: GradField::new(seed, seeds::LAKE_SHAPE),
            river_warp: GradField::new(seed, seeds::RIVER_WARP),
            river: GradField::new(seed, seeds::RIVER),
            finger: GradField::new(seed, seeds::FINGER),
        }
    }

    pub fn world_height(&self) -> i32 {
        self.world_height
    }

    /// Lac de la cellule `(i, j)` du Worley, s'il y en a un (doc 02 §6.3) : probabilité
    /// `0,15 + 0,6 · Lk`, centre sur la terre ferme (C ≥ 0,52), hors chaîne (U < 0,25), hors des
    /// auges, des plateaux et des parois. Niveau : un bloc sous le plus bas du relief et du relief
    /// filtré au centre, jamais sous la nappe `W`.
    pub fn lake_at_cell(&self, heights: &HeightFields, i: i32, j: i32) -> Option<Lake> {
        let center = self.lakes.point(i, j);
        let m = heights.macro_fields().sample(center[0], center[1]);
        if self.lakes.cell_rand(i, j, 0) >= 0.15 + 0.6 * m.lk || m.c < 0.52 || m.u >= 0.25 {
            return None;
        }
        let r = heights.relief(center[0], center[1], &m);
        if r.valley > 0.1 || r.hanging > 0.1 || r.plateau > 0.3 {
            return None;
        }
        let sea = f64::from(self.sea);
        let hl = filtered_height(&r, sea);
        let (w, _) = stair_level(hl, self.sea);
        let level = ((r.h.min(hl)).floor() as i32 - 1).max(w);
        if level <= self.sea {
            return None;
        }
        let a = self.lakes.cell_rand(i, j, 1);
        let radius = LAKE_RADIUS.0 + (LAKE_RADIUS.1 - LAKE_RADIUS.0) * a * a * a;
        let depth = 3.0 + 11.0 * self.lakes.cell_rand(i, j, 2);
        Some(Lake {
            center,
            radius,
            depth,
            level,
            axis: m.theta,
        })
    }

    /// Distance (en blocs, estimée au premier ordre) à l'axe de la rivière la plus proche :
    /// `Rv = Z(warp(p; 110, 300) / 850)` divisé par la norme de son gradient.
    pub fn river_distance(&self, x: f64, z: f64) -> f64 {
        const L: f64 = 850.0;
        let (px, pz) = self.river_warp.warp(x, z, 110.0, 300.0);
        let f = self.river.fbm(px / L, pz / L, 3);
        let fx = self.river.fbm((px + 1.0) / L, pz / L, 3);
        let fz = self.river.fbm(px / L, (pz + 1.0) / L, 3);
        let g = 2.0 * ((fx - f) * (fx - f) + (fz - f) * (fz - f)).sqrt();
        (2.0 * f - 1.0).abs() / g.max(1e-6)
    }

    /// Colonne d'eau **brute** (avant scellement) au point `(x, z)`, à partir de son relief `r`,
    /// de ses champs `m` et des lacs voisins.
    pub fn column(
        &self,
        x: f64,
        z: f64,
        r: &Relief,
        m: &MacroSample,
        lakes: &LakeSet,
    ) -> WaterColumn {
        let sea = f64::from(self.sea);
        let hl = filtered_height(r, sea);
        let (w, frac) = stair_level(hl, self.sea);
        let wf = f64::from(w);
        let mut h = r.h;
        let mut sill_raised = false;

        // 1. Juste au-dessus d'une marche de W, les creux sont comblés jusqu'à W.
        if w > self.sea && frac < STEP_SILL {
            sill_raised = h < wf - 1.0;
            h = h.max(wf);
        }

        // Auge glaciaire : poids g et position t dans l'auge (0 sur l'axe), comme en §5.4.
        let g = smooth(0.25, 0.50, m.u) * smooth(0.40, 0.44, m.c);
        let trough_t = if g > 0.05 {
            let u = (r.valley / g).min(1.0);
            (1.0 - u).sqrt().sqrt()
        } else {
            1.0
        };
        let inland_trough = g > 0.3 && r.valley_floor >= sea + 2.0;

        // 2. Lacs de vallée (fingersjöar) : tronçons d'auge intérieure surcreusés de 8 blocs ;
        //    le fond plat de l'auge (W + 2) les barre de part et d'autre.
        if inland_trough && trough_t < 0.5 {
            let f = spread(self.finger.fbm(x / 600.0, z / 600.0, 2));
            let k = smooth(0.6, 0.7, f) * smooth(0.5, 0.2, trough_t) * smooth(0.3, 0.5, g);
            if k > 0.0 {
                h = h.min(wf + 10.0 - 18.0 * k);
            }
        }

        // 3. Lacs.
        let mut level = w;
        let mut kind_lake = false;
        let mut inside: Option<(f64, &Lake)> = None;
        let mut rim = f64::NEG_INFINITY;
        let mut rim_level: Option<i32> = None;
        let mut carve = f64::INFINITY;
        for lake in lakes.near(x, z) {
            let d = lake.distance(x, z);
            if d >= lake.reach() {
                continue;
            }
            let shape = 0.6 + 0.8 * spread(self.lake_shape.fbm(x / 70.0, z / 70.0, 3));
            let rr = lake.radius * shape;
            let lf = f64::from(lake.level);
            if d < rr {
                let delta = d / rr;
                if inside.map_or(true, |(best, _)| delta < best) {
                    inside = Some((delta, lake));
                }
            } else {
                let out = d - rr;
                // Berge talutée, qui s'efface (pente plus forte) avant la limite de portée.
                let fade = (out - (LAKE_BANK_REACH - 10.0)).max(0.0) * 2.0;
                carve = carve.min(lf + LAKE_BANK_SLOPE * out + fade);
                rim = rim.max(lf - (out - RIM_WIDTH + 1.0).max(0.0));
                if out < RIM_WIDTH {
                    rim_level = Some(rim_level.map_or(lake.level, |l| l.max(lake.level)));
                }
            }
        }
        h = h.min(carve);
        if let Some((delta, lake)) = inside {
            h = h.min(f64::from(lake.level) - lake.depth * (1.0 - delta * delta));
            level = lake.level;
            kind_lake = true;
        } else {
            h = h.max(rim);
            if let Some(l) = rim_level {
                level = l;
            }
        }

        // 4. Rivières (hors lacs) : lit calé sur W, barre au-dessus des marches, bassin en aval.
        let river_width = 2.5 + 4.5 * smooth(0.70, 0.47, m.c);
        let river_distance = self.river_distance(x, z);
        let mut river = false;
        let mut channel_distance = f64::INFINITY;
        if inside.is_none() && rim_level.is_none() {
            let sill = w > self.sea && frac < STEP_SILL;
            let shape = |d: f64, width: f64| {
                let mut b = channel(d, width, wf);
                if sill {
                    b = b.max(wf - 1.0);
                } else if d < width {
                    b -= 3.0 * smooth(PLUNGE_FROM, 1.0, frac);
                }
                b
            };
            // Réseau de plaine, éteint dans la chaîne.
            let rho = 1.0 - smooth(0.20, 0.45, m.u);
            let mut target = shape(river_distance, river_width) + (1.0 - rho) * 80.0;
            if rho > 0.5 {
                channel_distance = river_distance;
            }
            // Dans la chaîne, la rivière emprunte le fond des auges.
            if inland_trough {
                let rho_v = smooth(0.3, 0.6, g);
                if rho_v > 0.5 {
                    channel_distance = channel_distance.min(trough_t * TROUGH_BLOCKS);
                }
                target = target.min(shape(trough_t * TROUGH_BLOCKS, 3.0) + (1.0 - rho_v) * 80.0);
            }
            if target < h {
                h = target;
                river = target < wf;
            }
        }

        let top = (h.floor() as i32).clamp(1, self.world_height - 1);
        let wet = level > top;
        let kind = if !wet {
            WaterKind::None
        } else if kind_lake {
            WaterKind::Lake
        } else if level == self.sea && (m.c < 0.47 || r.fjord || hl < sea - 2.0) {
            WaterKind::Sea
        } else if river {
            WaterKind::River
        } else if level == self.sea && m.c < 0.50 {
            WaterKind::Sea
        } else {
            WaterKind::Lake
        };
        WaterColumn {
            top,
            level,
            kind,
            stair: w,
            channel_distance,
            river_width,
            dam: sill_raised && !wet,
        }
    }
}

/// Relief et hydrographie d'un monde réunis : ce que voient le générateur, la hauteur isolée
/// (départ du joueur) et les cartes.
#[derive(Debug, Clone)]
pub struct Hydrography {
    heights: HeightFields,
    hydro: Hydrology,
}

impl Hydrography {
    pub fn new(seed: i64, cfg: NorrConfig) -> Self {
        Self {
            heights: HeightFields::new(seed, cfg),
            hydro: Hydrology::new(seed, cfg),
        }
    }

    pub fn heights(&self) -> &HeightFields {
        &self.heights
    }

    pub fn hydrology(&self) -> &Hydrology {
        &self.hydro
    }

    /// Lacs couvrant un rectangle de colonnes.
    pub fn lakes(&self, x_min: i32, z_min: i32, x_max: i32, z_max: i32) -> LakeSet {
        LakeSet::new(&self.hydro, &self.heights, x_min, z_min, x_max, z_max)
    }

    /// Colonne brute `(wx, wz)`, champs lents lus par le treillis 4 × 4 (comme les chunks).
    pub fn raw_column(&self, wx: i32, wz: i32, lakes: &LakeSet) -> WaterColumn {
        let m = self.heights.macro_fields().sample_lattice(wx, wz);
        let (x, z) = (f64::from(wx), f64::from(wz));
        let r = self.heights.relief(x, z, &m);
        self.hydro.column(x, z, &r, &m, lakes)
    }

    /// Colonne scellée `(wx, wz)` : **exactement** la colonne du chunk généré.
    pub fn column(&self, wx: i32, wz: i32) -> WaterColumn {
        let lakes = self.lakes(wx - 1, wz - 1, wx + 1, wz + 1);
        let c = self.raw_column(wx, wz, &lakes);
        let n = [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .map(|(dx, dz)| self.raw_column(wx + dx, wz + dz, &lakes));
        seal(c, n, self.hydro.world_height())
    }

    /// Colonne brute au point `(x, z)`, champs évalués exactement (cartes).
    pub fn column_exact(
        &self,
        x: f64,
        z: f64,
        lakes: &LakeSet,
    ) -> (Relief, MacroSample, WaterColumn) {
        let m = self.heights.macro_fields().sample(x, z);
        let r = self.heights.relief(x, z, &m);
        let c = self.hydro.column(x, z, &r, &m, lakes);
        (r, m, c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(top: i32, level: i32) -> WaterColumn {
        WaterColumn {
            top,
            level,
            kind: if level > top {
                WaterKind::Lake
            } else {
                WaterKind::None
            },
            stair: level,
            channel_distance: f64::INFINITY,
            river_width: 3.0,
            dam: false,
        }
    }

    #[test]
    fn stair_level_has_steps_of_six() {
        assert_eq!(stair_level(10.0, 40), (40, 0.0));
        assert_eq!(stair_level(47.9, 40).0, 40);
        assert_eq!(stair_level(48.0, 40), (46, 0.0));
        let (w, f) = stair_level(51.0, 40);
        assert_eq!(w, 46);
        assert!((f - 0.5).abs() < 1e-12);
        assert_eq!(stair_level(100.0, 40).0, 94);
    }

    #[test]
    fn channel_is_a_v_under_the_water_then_a_bank() {
        assert_eq!(channel(0.0, 4.0, 50.0), 45.0);
        assert_eq!(channel(4.0, 4.0, 50.0), 48.0);
        assert!(channel(6.5, 4.0, 50.0) >= 50.0);
        let mut prev = channel(0.0, 4.0, 50.0);
        for i in 1..200 {
            let v = channel(f64::from(i) * 0.1, 4.0, 50.0);
            assert!(v >= prev && v - prev < 0.2);
            prev = v;
        }
    }

    #[test]
    fn seal_raises_columns_next_to_higher_water() {
        let dry_low = col(44, 40);
        let lake = col(42, 46);
        let lower_water = col(38, 40);
        // Un sol plus bas que l'eau voisine devient un barrage au niveau de cette eau.
        let s = seal(dry_low, [lake, dry_low, dry_low, dry_low], 128);
        assert_eq!((s.top, s.dam, s.is_wet()), (46, true, false));
        // Une eau plus basse à côté d'une eau plus haute est remplacée par le barrage.
        let s = seal(lower_water, [lake, dry_low, dry_low, dry_low], 128);
        assert_eq!((s.top, s.dam, s.is_wet()), (46, true, false));
        // Même niveau, ou voisin assez haut : rien ne change.
        assert_eq!(seal(lake, [lake, col(50, 40), lake, lake], 128), lake);
    }
}
