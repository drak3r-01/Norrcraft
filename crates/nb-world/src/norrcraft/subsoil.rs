//! Sous-sol Norrcraft (session 2.2 ; GDD doc 02 §8) : strates et provinces rocheuses, filons de
//! diabase, minerais et cavités.
//!
//! Tout est une **fonction pure** de la graine et des coordonnées (aucune dépendance à l'ordre des
//! chunks) ; aucun appel transcendant (comme le reste de `norrcraft/`).
//!
//! # Strates (§8.1)
//!
//! De haut en bas : sol du biome et couverture meuble (`terrain.rs`, session 2.1), puis le **socle
//! de la province** de la colonne ([`Province`] : granit gris, gneiss, granit rouge, calcaire,
//! grès, schiste alunifère, grönsten, porfyr), **djupberg** sous `y < 14`, **bedrock** en `y ≤ 1`.
//! Le gneiss est obligatoire dans la chaîne (`U > 0,5`), le basalte remplace le socle du Lavafält,
//! la craie (**krita**) de Klintkust porte des bandes de **flinta**. Les **filons de diabase** sont
//! des murs verticaux de 2 blocs (`Z(p/900) < 0,004`) qui traversent tout le socle.
//!
//! # Minerais (§8.2)
//!
//! * en plaques : myrmalm (Myr, sous la tourbe), sjömalm (fond des lacs de Sjöland/Tallhed,
//!   Bottenhav peu profond) ;
//! * en filons de nappes de bruit 3D : fer de roche (Bergslag, grönsten, contact calcaire), argent
//!   (calcaire), quartz et or du nord ;
//! * en corps : Malmberg (lentille ellipsoïdale inclinée, affleurante, pas 512), cuivre (amas de
//!   pas 384 dans le pays minier), cobalt (poches), tungstène (sous Kalfjäll), stéatite
//!   (lentilles de la lande d'altitude) ;
//! * en surface : or des graviers de rivières du nord, soufre du Lavafält, quartz, argile et sable
//!   de verrerie.
//!
//! # Cavités (§8.3)
//!
//! Karst (calcaire), failles (granit et gneiss), grottes marines (falaises au niveau de la mer),
//! tunnels de lave (Lavafält, avec effondrements), grottes de glace (Glaciär), marmites de géant
//! (lits de rivière et côtes), salles de la montagne (chaîne). **Règle d'étanchéité unique** : une
//! cavité creusée au-dessous du niveau de la mer est noyée, au-dessus elle est d'air ; une
//! cavité enterrée garde 4 blocs de roche au-dessus d'elle et sous les colonnes voisines, donc
//! aucun bloc d'eau n'y touche un bloc d'air.

use nb_data::{BlockId, BlockRegistry};

use super::biomes::NorrBiome;
use super::climate::altitudes;
use super::config::NorrConfig;
use super::fields::{MacroFields, MacroSample, Province};
use super::hydro::{WaterColumn, WaterKind};
use super::math::{hash01, spread, GradField, GradField3};

/// Graines des champs du sous-sol (`graine XOR constante`), famille `0x…05xx`.
mod seeds {
    pub const DIKE: u64 = 0x4E52_5743_0000_0501;
    pub const FAULT: u64 = 0x4E52_5743_0000_0502;
    pub const FAULT_DEPTH: u64 = 0x4E52_5743_0000_0503;
    pub const KARST_A: u64 = 0x4E52_5743_0000_0504;
    pub const KARST_B: u64 = 0x4E52_5743_0000_0505;
    pub const IRON_A: u64 = 0x4E52_5743_0000_0506;
    pub const IRON_B: u64 = 0x4E52_5743_0000_0507;
    pub const SILVER: u64 = 0x4E52_5743_0000_0508;
    pub const QUARTZ: u64 = 0x4E52_5743_0000_0509;
    pub const GOLD: u64 = 0x4E52_5743_0000_050A;
    pub const WOBBLE: u64 = 0x4E52_5743_0000_050B;
    pub const FLINT: u64 = 0x4E52_5743_0000_050C;
    pub const BOG: u64 = 0x4E52_5743_0000_050D;
    pub const LAKE: u64 = 0x4E52_5743_0000_050E;
    pub const LAVA_LINE: u64 = 0x4E52_5743_0000_050F;
    pub const LAVA_SHAFT: u64 = 0x4E52_5743_0000_0510;
    pub const SULPHUR: u64 = 0x4E52_5743_0000_0511;
    pub const PLAQUE: u64 = 0x4E52_5743_0000_0512;
    pub const ICE_LINE: u64 = 0x4E52_5743_0000_0513;
    pub const GRID: u64 = 0x4E52_5743_0000_0514;
}

/// Gneiss obligatoire dans la chaîne : au-delà de cette surrection `U`.
const CHAIN_GNEISS_U: f64 = 0.8;
/// Profondeur (y) sous laquelle le socle devient djupberg.
pub const DEEP_Y: i32 = 14;
/// Épaisseur de roche gardée au-dessus d'une cavité enterrée.
pub const ROOF: i32 = 4;

// Filons de diabase : murs de 2 blocs (distance à la ligne de zéro < 1).
const DIKE_WAVELENGTH: f64 = 900.0;
const DIKE_HALF_WIDTH: f64 = 1.0;
// Failles.
const FAULT_WAVELENGTH: f64 = 600.0;
const FAULT_HALF_WIDTH: f64 = 0.8;
const FAULT_DEPTH_MIN: f64 = 10.0;
const FAULT_DEPTH_RANGE: f64 = 15.0;
// Karst : deux nappes de bruit 3D croisées, aplaties en hauteur (galeries horizontales).
const KARST_SCALE: [f64; 3] = [30.0, 18.0, 30.0];
const KARST_WIDTH: f64 = 0.12;
const KARST_Y_MIN: i32 = 5;
// Fer de roche, argent, quartz aurifère.
const IRON_Y: (i32, i32) = (10, 60);
const IRON_WIDTH: f64 = 0.035;
const IRON_RICH: f64 = 0.012;
const SILVER_Y: (i32, i32) = (5, 30);
const SILVER_WIDTH: f64 = 0.010;
const GOLD_Y_MAX: i32 = 24;
const GOLD_WIDTH: f64 = 0.02;
/// Au-dessous de ce T₀, l'or du nord (veines de quartz, paillettes des rivières).
const NORTH_T0: f64 = 0.28;
// Plaques de surface.
/// Plaques de minerai des marais et des lacs : `q(F(p/25)) > 0,68` et `q(F(p/20)) > 0,70`
/// (le fBm brut dépasse rarement 0,68 : on lit F étalé par `q`, comme les champs du §4).
const BOG_THRESHOLD: f64 = 0.68;
const LAKE_THRESHOLD: f64 = 0.70;
/// Probabilité de paillette d'or dans le gravier d'une rivière du nord.
const PLACER_SHARE: f64 = 0.04;
/// Part de Bottenhav « peu profond » (profondeur d'eau maximale, en blocs) qui porte du sjömalm.
const BOTTEN_SHALLOW: i32 = 8;
/// Hauteur du calcaire en craie à Klintkust : au-dessus de `S − CHALK_BELOW`.
const CHALK_BELOW: i32 = 4;

// Corps de minerai : pas des grilles (blocs).
const MALM_PITCH: i32 = 512;
const MALM_AXES: [f64; 3] = [60.0, 14.0, 45.0];
const MALM_TILT_COS: f64 = 0.5;
const MALM_TILT_SIN: f64 = 0.866_025_403_784_438_6;
/// Un Malmberg ne naît que si `T₀` du centre est sous ce seuil (boréal froid et subarctique)…
const MALM_T0_MAX: f64 = 0.34;
/// …et si la surrection `U` y dépasse ce seuil (la chaîne et ses contreforts).
const MALM_U_MIN: f64 = 0.20;
const COPPER_PITCH: i32 = 384;
const COBALT_PITCH: i32 = 64;
const TUNGSTEN_PITCH: i32 = 64;
const SOAP_PITCH: i32 = 128;
const SEA_CAVE_PITCH: i32 = 48;
const HALL_PITCH: i32 = 700;

/// Blocs du sous-sol, lus dans le registre du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubsoilBlocks {
    pub bedrock: BlockId,
    pub deep: BlockId,
    pub water: BlockId,
    pub diabase: BlockId,
    pub basalt: BlockId,
    pub chalk: BlockId,
    pub flint: BlockId,
    pub soapstone: BlockId,
    pub quartz: BlockId,
    pub sulphur: BlockId,
    pub red_earth: BlockId,
    pub clay: BlockId,
    pub glass_sand: BlockId,
    pub bog_ore: BlockId,
    pub lake_ore: BlockId,
    pub iron: BlockId,
    pub rich_iron: BlockId,
    pub copper: BlockId,
    pub silver: BlockId,
    pub cobalt: BlockId,
    pub gold_quartz: BlockId,
    pub gold_gravel: BlockId,
    pub tungsten: BlockId,
    /// Roche de socle de chaque province, dans l'ordre de [`Province::ALL`].
    pub provinces: [BlockId; 8],
}

impl SubsoilBlocks {
    pub fn from_registry(r: &BlockRegistry) -> Self {
        let id = |n: &str| {
            let b = r.get(n);
            assert!(b.name == n, "sous-sol : bloc « {n} » absent de Norrcraft");
            b.id
        };
        Self {
            bedrock: id("bedrock"),
            deep: id("djupberg"),
            water: id("water"),
            diabase: id("diabas"),
            basalt: id("basaltsten"),
            chalk: id("krita"),
            flint: id("flinta"),
            soapstone: id("taljsten"),
            quartz: id("kvarts"),
            sulphur: id("svavel"),
            red_earth: id("rodmull"),
            clay: id("blalera"),
            glass_sand: id("flygsand"),
            bog_ore: id("myrmalm"),
            lake_ore: id("sjomalm"),
            iron: id("jarnmalm"),
            rich_iron: id("rik_jarnmalm"),
            copper: id("kopparmalm"),
            silver: id("silvermalm"),
            cobalt: id("koboltmalm"),
            gold_quartz: id("guldkvarts"),
            gold_gravel: id("guldgrus"),
            tungsten: id("volframmalm"),
            provinces: [
                id("granit"),
                id("gnejs"),
                id("rodgranit"),
                id("kalksten"),
                id("sandsten"),
                id("alunskiffer"),
                id("gronsten"),
                id("porfyr"),
            ],
        }
    }

    /// Roche de socle d'une province.
    pub fn rock_of(&self, p: Province) -> BlockId {
        self.provinces[p as usize]
    }
}

// ---------------------------------------------------------------------------------------------
// Corps (lentilles, amas, poches, salles)
// ---------------------------------------------------------------------------------------------

/// Nature d'un corps de la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyKind {
    /// Montagne de fer : jarnmalm, cœur riche.
    Malmberg,
    /// Amas de cuivre : kopparmalm, auréole de terre rouge.
    Copper,
    Cobalt,
    Tungsten,
    Soapstone,
    /// Grotte marine (creusée).
    SeaCave,
    /// Salle de la montagne (creusée).
    Hall,
}

/// Ellipsoïde orienté `Σ ((P − c)·eₖ / aₖ)² < 1 + w · N₃(P/12)`.
#[derive(Debug, Clone, Copy)]
struct Body {
    kind: BodyKind,
    c: [f64; 3],
    e: [[f64; 3]; 3],
    a: [f64; 3],
    /// Amplitude de l'irrégularité de la paroi.
    wobble: f64,
    /// Rayon englobant (blocs) : test rapide.
    reach: f64,
}

impl Body {
    fn sphere(kind: BodyKind, c: [f64; 3], r: f64, wobble: f64) -> Self {
        Self {
            kind,
            c,
            e: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            a: [r, r, r],
            wobble,
            reach: r * (1.0 + wobble).sqrt() + 1.0,
        }
    }

    /// `Σ ((P − c)·eₖ / aₖ)²`, ou `None` hors du rayon englobant.
    fn value(&self, p: [f64; 3]) -> Option<f64> {
        let d = [p[0] - self.c[0], p[1] - self.c[1], p[2] - self.c[2]];
        if d[0].abs() > self.reach || d[1].abs() > self.reach || d[2].abs() > self.reach {
            return None;
        }
        let mut s = 0.0;
        for k in 0..3 {
            let t = (d[0] * self.e[k][0] + d[1] * self.e[k][1] + d[2] * self.e[k][2]) / self.a[k];
            s += t * t;
        }
        Some(s)
    }
}

/// Distance estimée (en blocs) à la ligne de zéro de `2 F(p / ℓ) − 1` : `|s| / |∇s|`, gradient par
/// différences centrées. `+∞` loin de la ligne (le calcul du gradient est alors évité).
fn zero_distance(field: &GradField, x: f64, z: f64, wavelength: f64, octaves: u32) -> f64 {
    let s = |dx: f64, dz: f64| {
        2.0 * field.fbm((x + dx) / wavelength, (z + dz) / wavelength, octaves) - 1.0
    };
    let s0 = s(0.0, 0.0);
    if s0.abs() > 0.06 {
        return f64::INFINITY;
    }
    let gx = (s(1.0, 0.0) - s(-1.0, 0.0)) * 0.5;
    let gz = (s(0.0, 1.0) - s(0.0, -1.0)) * 0.5;
    let g = (gx * gx + gz * gz).sqrt();
    if g < 1e-9 {
        f64::INFINITY
    } else {
        s0.abs() / g
    }
}

/// Vecteur horizontal unitaire tiré de deux hachages (sans transcendante).
fn unit_xz(seed: u64, i: i32, j: i32, salt: i32) -> [f64; 2] {
    let a = 2.0 * hash01(seed, i, j, salt) - 1.0;
    let b = 2.0 * hash01(seed, i, j, salt + 1) - 1.0;
    let n = (a * a + b * b).sqrt();
    if n < 1e-6 {
        [1.0, 0.0]
    } else {
        [a / n, b / n]
    }
}

/// Centre jitteré de la cellule `(i, j)` d'une grille de pas `pitch`.
fn cell_center(seed: u64, pitch: i32, i: i32, j: i32) -> (i32, i32) {
    let p = f64::from(pitch);
    let x = (f64::from(i) + 0.1 + 0.8 * hash01(seed, i, j, 1)) * p;
    let z = (f64::from(j) + 0.1 + 0.8 * hash01(seed, i, j, 2)) * p;
    (x.floor() as i32, z.floor() as i32)
}

/// Rectangle de colonnes `[x0, x1] × [z0, z1]` (un chunk, ou une région de carte).
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
}

impl Region {
    /// Le chunk d'origine `(x0, z0)`.
    pub fn chunk(x0: i32, z0: i32) -> Self {
        Self {
            x0,
            z0,
            x1: x0 + 15,
            z1: z0 + 15,
        }
    }
}

/// Cellules d'une grille de pas `pitch` dont le corps (rayon `reach`) peut atteindre la région.
fn cells_near(pitch: i32, reg: Region, reach: f64) -> impl Iterator<Item = (i32, i32)> {
    let r = reach.ceil() as i32;
    let (i0, i1) = (
        (reg.x0 - r).div_euclid(pitch),
        (reg.x1 + r).div_euclid(pitch),
    );
    let (j0, j1) = (
        (reg.z0 - r).div_euclid(pitch),
        (reg.z1 + r).div_euclid(pitch),
    );
    (j0..=j1).flat_map(move |j| (i0..=i1).map(move |i| (i, j)))
}

/// Ce que le sous-sol demande au terrain pour placer ses corps : quatre sondes pures.
pub struct SiteProbe<'a> {
    /// Premier `y` non solide de la colonne (sol, ou fond sous l'eau).
    pub surface: &'a dyn Fn(i32, i32) -> i32,
    /// Colonne d'eau scellée.
    pub water: &'a dyn Fn(i32, i32) -> WaterColumn,
    /// Champs macroscopiques de la colonne.
    pub macros: &'a dyn Fn(i32, i32) -> MacroSample,
    /// Biome de la colonne.
    pub biome: &'a dyn Fn(i32, i32) -> NorrBiome,
}

/// Corps qui touchent un chunk.
#[derive(Debug, Clone, Default)]
pub struct Sites {
    bodies: Vec<Body>,
}

impl Sites {
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    pub fn len(&self) -> usize {
        self.bodies.len()
    }
}

/// Un corps de minerai (centre `x, z`, nature) pour les cartes et les tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepositInfo {
    pub kind: &'static str,
    pub center: [i32; 3],
    pub radius: f64,
}

// ---------------------------------------------------------------------------------------------
// Colonne
// ---------------------------------------------------------------------------------------------

/// Données d'entrée d'une colonne pour [`Subsoil::column`].
pub struct ColumnIn<'a> {
    pub wx: i32,
    pub wz: i32,
    /// Eau scellée de la colonne.
    pub water: WaterColumn,
    /// Eau scellée des 4 voisines (−x, +x, −z, +z).
    pub neighbours: [WaterColumn; 4],
    pub biome: NorrBiome,
    pub fresh_water: bool,
    /// La peau de la colonne est de la neige : pas de plaque ni de chapeau de fer par-dessus.
    pub snow: bool,
    pub m: &'a MacroSample,
    /// Province de la colonne et son poids (1 loin des frontières).
    pub province: (Province, f64),
    /// Premier `y` du socle : tout `y < cover_bottom` est du socle ; au-dessus, la couverture.
    pub cover_bottom: i32,
}

/// Résultat par colonne du sous-sol : fixe tous les tirages 2D, une fois pour les `h` cellules.
#[derive(Debug, Clone, Copy)]
pub struct SubColumn {
    wx: i32,
    wz: i32,
    pub province: Province,
    rock: BlockId,
    /// Premier `y` non solide de la colonne.
    top: i32,
    cover_bottom: i32,
    /// Dernier `y` qu'une cavité enterrée peut creuser (`min(tops) − ROOF`).
    ceiling: i32,
    dike: bool,
    /// Profondeur de la faille ouverte dans la colonne (0 = aucune).
    fault: i32,
    karst: bool,
    klint: bool,
    /// Pays minier : grönsten, Bergslag, contact avec une province calcaire.
    mining: bool,
    north: bool,
    bog_ore: bool,
    lake_ore: bool,
    gold_placer: bool,
    /// Plaque de surface (soufre, quartz) à poser en `top − 1`.
    plaque: Option<BlockId>,
    /// Plaque d'argile ou de sable de verrerie, sous la peau (de `top − 3` à `top − 2`).
    under_plaque: Option<BlockId>,
    /// Tube de lave : `y` central du tube dans la colonne (le tube remplit `±2`).
    lava_tube: Option<i32>,
    /// Effondrement de la voûte du tube.
    lava_shaft: bool,
    /// Voisines toutes sèches ou marines (grottes marines).
    coast_ok: bool,
    /// Tunnel de fonte dans la glace d'un glacier.
    ice_tunnel: bool,
    /// Profondeur d'une marmite de géant dans cette colonne (0 = aucune).
    pothole: i32,
    wet: bool,
    /// Chapeau de fer : terre rouille en surface au-dessus d'un filon proche.
    cap: bool,
}

impl SubColumn {
    /// Roche de socle de la colonne (avant tout minerai ou cavité).
    pub fn rock(&self) -> BlockId {
        self.rock
    }

    /// Profondeur de la faille de la colonne, 0 sans faille.
    pub fn fault_depth(&self) -> i32 {
        self.fault
    }

    pub fn has_dike(&self) -> bool {
        self.dike
    }
}

/// Générateur du sous-sol : champs de bruit et blocs. Fonction pure de la graine.
#[derive(Debug, Clone)]
pub struct Subsoil {
    sea: i32,
    blocks: SubsoilBlocks,
    fields: MacroFields,
    dike: GradField,
    fault: GradField,
    fault_depth: GradField,
    bog: GradField,
    lake: GradField,
    lava_line: GradField,
    lava_shaft: GradField,
    sulphur: GradField,
    plaque: GradField,
    ice_line: GradField,
    karst_a: GradField3,
    karst_b: GradField3,
    iron_a: GradField3,
    iron_b: GradField3,
    silver: GradField3,
    quartz: GradField3,
    gold: GradField3,
    wobble: GradField3,
    flint: GradField3,
    grid_seed: u64,
}

impl Subsoil {
    pub fn new(seed: i64, cfg: NorrConfig, blocks: SubsoilBlocks) -> Self {
        let f2 = |c: u64| GradField::new(seed, c);
        let f3 = |c: u64| GradField3::new(seed, c);
        Self {
            sea: cfg.sea_level,
            blocks,
            fields: MacroFields::new(seed, cfg),
            dike: f2(seeds::DIKE),
            fault: f2(seeds::FAULT),
            fault_depth: f2(seeds::FAULT_DEPTH),
            bog: f2(seeds::BOG),
            lake: f2(seeds::LAKE),
            lava_line: f2(seeds::LAVA_LINE),
            lava_shaft: f2(seeds::LAVA_SHAFT),
            sulphur: f2(seeds::SULPHUR),
            plaque: f2(seeds::PLAQUE),
            ice_line: f2(seeds::ICE_LINE),
            karst_a: f3(seeds::KARST_A),
            karst_b: f3(seeds::KARST_B),
            iron_a: f3(seeds::IRON_A),
            iron_b: f3(seeds::IRON_B),
            silver: f3(seeds::SILVER),
            quartz: f3(seeds::QUARTZ),
            gold: f3(seeds::GOLD),
            wobble: f3(seeds::WOBBLE),
            flint: f3(seeds::FLINT),
            grid_seed: (seed as u64) ^ seeds::GRID,
        }
    }

    pub fn blocks(&self) -> &SubsoilBlocks {
        &self.blocks
    }

    /// Vrai si la colonne est sur un filon de diabase (mur de 2 blocs qui traverse le socle).
    pub fn dike_at(&self, wx: i32, wz: i32) -> bool {
        zero_distance(&self.dike, f64::from(wx), f64::from(wz), DIKE_WAVELENGTH, 3)
            < DIKE_HALF_WIDTH
    }

    /// Vrai si la colonne est sur la ligne d'une faille (avant les conditions de roche et de voisinage).
    pub fn fault_line_at(&self, wx: i32, wz: i32) -> bool {
        zero_distance(
            &self.fault,
            f64::from(wx),
            f64::from(wz),
            FAULT_WAVELENGTH,
            3,
        ) < FAULT_HALF_WIDTH
    }

    /// Province de la colonne (champs lus au treillis), pour les cartes et les tests.
    pub fn province_at(&self, wx: i32, wz: i32) -> (Province, f64) {
        self.province(wx, wz, &self.fields.sample_lattice(wx, wz))
    }

    // -----------------------------------------------------------------------------------------
    // Provinces
    // -----------------------------------------------------------------------------------------

    /// Province de la colonne `(wx, wz)` et son poids : lue sur un treillis global de 4 blocs
    /// (réévaluée exactement quand les quatre coins diffèrent, ou en pays calcaire pour le poids),
    /// gneiss dans la chaîne (`U > 0,5`).
    pub fn province(&self, wx: i32, wz: i32, m: &MacroSample) -> (Province, f64) {
        if m.u > CHAIN_GNEISS_U {
            return (Province::Gneiss, 1.0);
        }
        let (i, j) = (wx.div_euclid(4), wz.div_euclid(4));
        let at = |a: i32, b: i32| self.fields.province(f64::from(a * 4), f64::from(b * 4));
        let corners = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
        let uniform = corners.iter().all(|&p| p == corners[0]);
        if uniform && corners[0] != Province::Limestone {
            return (corners[0], 1.0);
        }
        self.fields.province_blend(f64::from(wx), f64::from(wz))
    }

    // -----------------------------------------------------------------------------------------
    // Colonne
    // -----------------------------------------------------------------------------------------

    pub fn column(&self, c: &ColumnIn) -> SubColumn {
        let (wx, wz) = (c.wx, c.wz);
        let (x, z) = (f64::from(wx), f64::from(wz));
        let b = &self.blocks;
        let w = &c.water;
        let (province, weight) = c.province;
        let biome = c.biome;
        let h = w.top;
        let wet = w.is_wet();
        let tops = [
            c.neighbours[0].top,
            c.neighbours[1].top,
            c.neighbours[2].top,
            c.neighbours[3].top,
        ];
        let min_top = tops.iter().copied().fold(h, i32::min);
        let dry_around = !wet && c.neighbours.iter().all(|n| !n.is_wet());
        let coast_ok = c
            .neighbours
            .iter()
            .all(|n| !n.is_wet() || n.kind == WaterKind::Sea);

        let rock = if biome == NorrBiome::Lavafalt {
            b.basalt
        } else {
            b.rock_of(province)
        };
        let granitic = matches!(
            province,
            Province::GraniteGrey | Province::GraniteRed | Province::Gneiss
        );
        let lava = biome == NorrBiome::Lavafalt;
        let klint = biome == NorrBiome::Klintkust;

        let dike = zero_distance(&self.dike, x, z, DIKE_WAVELENGTH, 3) < DIKE_HALF_WIDTH;
        let fault = if granitic && !wet && !dike && dry_around && !lava {
            if zero_distance(&self.fault, x, z, FAULT_WAVELENGTH, 3) < FAULT_HALF_WIDTH {
                let d =
                    FAULT_DEPTH_MIN + FAULT_DEPTH_RANGE * self.fault_depth.n(x / 37.0, z / 37.0);
                (d.floor() as i32).min(h - 4)
            } else {
                0
            }
        } else {
            0
        };
        let karst = province == Province::Limestone && !lava;
        let mining = province == Province::Greenstone
            || biome == NorrBiome::Bergslag
            || (province == Province::Limestone && weight < 0.5);
        let cap = mining && !wet && !c.snow && self.iron_cap(wx, wz, h);
        let north = c.m.t0 < NORTH_T0;

        let bog_ore = biome == NorrBiome::Myr
            && !wet
            && spread(self.bog.fbm(x / 25.0, z / 25.0, 3)) > BOG_THRESHOLD;
        let shallow_sea = biome == NorrBiome::Bottenhav && w.level - w.top <= BOTTEN_SHALLOW;
        let lake_ore = wet
            && ((w.kind == WaterKind::Lake
                && matches!(biome, NorrBiome::Sjoland | NorrBiome::Tallhed))
                || (c.fresh_water && matches!(biome, NorrBiome::Sjoland | NorrBiome::Tallhed))
                || shallow_sea)
            && spread(self.lake.fbm(x / 20.0, z / 20.0, 3)) > LAKE_THRESHOLD;
        let gold_placer = wet
            && w.kind == WaterKind::River
            && north
            && hash01(self.grid_seed, wx, wz, 7) < PLACER_SHARE;

        // Plaques de surface (soufre du Lavafält, quartz des roches nues) et sous la peau (argile,
        // sable de verrerie).
        let mut plaque = None;
        let mut under_plaque = None;
        if !wet && !c.snow {
            if lava && c.m.gt > 0.5 && spread(self.sulphur.fbm(x / 14.0, z / 14.0, 2)) > 0.86 {
                plaque = Some(b.sulphur);
            } else if matches!(
                biome,
                NorrBiome::Skargard
                    | NorrBiome::Fjallhed
                    | NorrBiome::Kalfjall
                    | NorrBiome::Hogkust
            ) && spread(self.plaque.fbm(x / 20.0, z / 20.0, 2)) > 0.82
            {
                plaque = Some(b.quartz);
            }
            if matches!(
                biome,
                NorrBiome::Hagmark | NorrBiome::Bokskog | NorrBiome::Strandang
            ) && c.m.e > 0.55
                && self.lake.fbm(x / 30.0 + 7.0, z / 30.0 + 3.0, 2) > 0.62
            {
                under_plaque = Some(b.clay);
            } else if biome == NorrBiome::Sjoland
                && self.lake.fbm(x / 26.0 + 11.0, z / 26.0 + 5.0, 2) > 0.66
            {
                under_plaque = Some(b.glass_sand);
            }
        }

        // Tubes de lave : ligne de zéro méandreuse, à profondeur à peu près constante sous le sol
        // (le tube suit la pente), effondrements ponctuels.
        let mut lava_tube = None;
        let mut lava_shaft = false;
        if lava && !wet && zero_distance(&self.lava_line, x, z, 60.0, 2) < 1.5 {
            let sink = (self.lava_shaft.n(x / 40.0, z / 40.0) * 2.0).floor() as i32;
            let yc = h - 8 - sink;
            if yc - 2 >= 3 {
                lava_tube = Some(yc);
                lava_shaft = dry_around && self.lava_shaft.n(x / 5.0 + 3.0, z / 5.0) > 0.88;
            }
        }

        // Tunnel de fonte à la base d'un glacier (sec, voisines sèches).
        let ice_tunnel =
            dry_around && zero_distance(&self.ice_line, x, z, 45.0, 2) < 1.5 && c.m.t0 < 0.5;

        // Marmite de géant : puits de 2 × 5 blocs, par grille de pas 24.
        let pothole = self.pothole_depth(wx, wz, w, province, coast_ok);

        SubColumn {
            wx,
            wz,
            province,
            rock,
            top: h,
            cover_bottom: c.cover_bottom,
            ceiling: min_top - ROOF,
            dike,
            fault,
            karst,
            klint,
            mining,
            north,
            bog_ore,
            lake_ore,
            gold_placer,
            plaque,
            under_plaque,
            lava_tube,
            lava_shaft,
            coast_ok,
            ice_tunnel,
            pothole,
            wet,
            cap,
        }
    }

    /// Profondeur de la marmite de géant qui occupe la colonne, 0 si aucune. Les marmites naissent
    /// dans les lits de rivière (noyées) et sur les rochers du rivage marin (hors province
    /// calcaire : le karst a son propre niveau d'eau).
    fn pothole_depth(
        &self,
        wx: i32,
        wz: i32,
        w: &WaterColumn,
        province: Province,
        coast_ok: bool,
    ) -> i32 {
        const PITCH: i32 = 24;
        let (i, j) = (wx.div_euclid(PITCH), wz.div_euclid(PITCH));
        if hash01(self.grid_seed, i, j, 11) >= 0.22 {
            return 0;
        }
        // Puits 2 × 2 de coin `(px, pz)`.
        let px = i * PITCH + 2 + (hash01(self.grid_seed, i, j, 12) * f64::from(PITCH - 6)) as i32;
        let pz = j * PITCH + 2 + (hash01(self.grid_seed, i, j, 13) * f64::from(PITCH - 6)) as i32;
        if wx < px || wx > px + 1 || wz < pz || wz > pz + 1 {
            return 0;
        }
        let river = w.is_wet() && w.kind == WaterKind::River;
        let shore = !w.is_wet()
            && w.top >= self.sea
            && w.top <= self.sea + 2
            && coast_ok
            && province != Province::Limestone;
        if river || shore {
            5
        } else {
            0
        }
    }

    // -----------------------------------------------------------------------------------------
    // Corps de la grille
    // -----------------------------------------------------------------------------------------

    /// Corps (lentilles, amas, poches, grottes, salles) qui touchent la région `reg`.
    pub fn sites(&self, reg: Region, probe: &SiteProbe) -> Sites {
        let mut bodies = Vec::new();
        self.malmbergs(reg, probe, &mut bodies);
        self.coppers(reg, probe, &mut bodies);
        self.pockets(reg, probe, &mut bodies);
        self.soapstones(reg, probe, &mut bodies);
        self.sea_caves(reg, probe, &mut bodies);
        self.halls(reg, probe, &mut bodies);
        Sites { bodies }
    }

    /// Corps de la région `reg`, avec leur description : pour les cartes et les tests (mêmes corps
    /// que ceux que le générateur creuse).
    pub fn deposits(&self, reg: Region, probe: &SiteProbe) -> Vec<DepositInfo> {
        self.sites(reg, probe)
            .bodies
            .iter()
            .map(|b| DepositInfo {
                kind: match b.kind {
                    BodyKind::Malmberg => "malmberg",
                    BodyKind::Copper => "cuivre",
                    BodyKind::Cobalt => "cobalt",
                    BodyKind::Tungsten => "tungstène",
                    BodyKind::Soapstone => "stéatite",
                    BodyKind::SeaCave => "grotte marine",
                    BodyKind::Hall => "salle de la montagne",
                },
                center: [b.c[0] as i32, b.c[1] as i32, b.c[2] as i32],
                radius: b.reach,
            })
            .collect()
    }

    /// Un Malmberg : une cellule de pas 512 sur deux, lentille de demi-axes (60, 14, 45) inclinée
    /// à 60°, enfoncée de 8 blocs sous le sol pour affleurer.
    pub fn malmberg_at_cell(&self, i: i32, j: i32, probe: &SiteProbe) -> Option<(i32, i32, i32)> {
        if hash01(self.grid_seed, i, j, 21) >= 0.5 {
            return None;
        }
        let (cx, cz) = cell_center(self.grid_seed ^ 0x21, MALM_PITCH, i, j);
        let m = (probe.macros)(cx, cz);
        if m.t0 >= MALM_T0_MAX || m.u <= MALM_U_MIN || !m.is_land() {
            return None;
        }
        let w = (probe.water)(cx, cz);
        if w.is_wet() {
            return None;
        }
        Some((cx, w.top - 8, cz))
    }

    fn malmbergs(&self, reg: Region, probe: &SiteProbe, out: &mut Vec<Body>) {
        for (i, j) in cells_near(MALM_PITCH, reg, 80.0) {
            let Some((cx, cy, cz)) = self.malmberg_at_cell(i, j, probe) else {
                continue;
            };
            let u = unit_xz(self.grid_seed, i, j, 22);
            let e0 = [u[0], 0.0, u[1]];
            let p = [-u[1], 0.0, u[0]];
            let e2 = [p[0] * MALM_TILT_COS, MALM_TILT_SIN, p[2] * MALM_TILT_COS];
            let e1 = [-p[0] * MALM_TILT_SIN, MALM_TILT_COS, -p[2] * MALM_TILT_SIN];
            out.push(Body {
                kind: BodyKind::Malmberg,
                c: [f64::from(cx) + 0.5, f64::from(cy), f64::from(cz) + 0.5],
                e: [e0, e1, e2],
                a: MALM_AXES,
                wobble: 0.25,
                reach: 76.0,
            });
        }
    }

    /// Cellule de cuivre `(i, j)` : amas de rayon 12 à 20 dans le pays minier.
    fn coppers(&self, reg: Region, probe: &SiteProbe, out: &mut Vec<Body>) {
        for (i, j) in cells_near(COPPER_PITCH, reg, 26.0) {
            if hash01(self.grid_seed, i, j, 31) >= 0.6 {
                continue;
            }
            let (cx, cz) = cell_center(self.grid_seed ^ 0x31, COPPER_PITCH, i, j);
            if !self.mining_site(cx, cz, probe) {
                continue;
            }
            let w = (probe.water)(cx, cz);
            let r = 12.0 + 8.0 * hash01(self.grid_seed, i, j, 32);
            let cy = f64::from(w.top) - 10.0 - 6.0 * hash01(self.grid_seed, i, j, 33);
            if cy - r < 3.0 {
                continue;
            }
            out.push(Body::sphere(
                BodyKind::Copper,
                [f64::from(cx) + 0.5, cy, f64::from(cz) + 0.5],
                r,
                0.3,
            ));
        }
    }

    fn mining_site(&self, cx: i32, cz: i32, probe: &SiteProbe) -> bool {
        let m = (probe.macros)(cx, cz);
        if !m.is_land() {
            return false;
        }
        let (p, _) = self.province(cx, cz, &m);
        p == Province::Greenstone || (probe.biome)(cx, cz) == NorrBiome::Bergslag
    }

    /// Cobalt (poches de 3 à 6 blocs, pays minier, y < 35) et tungstène (poches de 2 à 4 blocs,
    /// y < 18, sous Kalfjäll).
    fn pockets(&self, reg: Region, probe: &SiteProbe, out: &mut Vec<Body>) {
        for (i, j) in cells_near(COBALT_PITCH, reg, 5.0) {
            if hash01(self.grid_seed, i, j, 41) < 0.10 {
                let (cx, cz) = cell_center(self.grid_seed ^ 0x41, COBALT_PITCH, i, j);
                if self.mining_site(cx, cz, probe) {
                    let cy = 8.0 + 26.0 * hash01(self.grid_seed, i, j, 42);
                    let r = 1.5 + 1.5 * hash01(self.grid_seed, i, j, 43);
                    out.push(Body::sphere(
                        BodyKind::Cobalt,
                        [f64::from(cx) + 0.5, cy, f64::from(cz) + 0.5],
                        r,
                        0.3,
                    ));
                }
            }
        }
        for (i, j) in cells_near(TUNGSTEN_PITCH, reg, 4.0) {
            if hash01(self.grid_seed, i, j, 51) < 0.12 {
                let (cx, cz) = cell_center(self.grid_seed ^ 0x51, TUNGSTEN_PITCH, i, j);
                let m = (probe.macros)(cx, cz);
                let w = (probe.water)(cx, cz);
                if m.is_land()
                    && !w.is_wet()
                    && f64::from(w.top) > altitudes(m.t0, self.sea).bare_rock
                {
                    let cy = 4.0 + 13.0 * hash01(self.grid_seed, i, j, 52);
                    let r = 1.0 + 1.0 * hash01(self.grid_seed, i, j, 53);
                    out.push(Body::sphere(
                        BodyKind::Tungsten,
                        [f64::from(cx) + 0.5, cy, f64::from(cz) + 0.5],
                        r,
                        0.3,
                    ));
                }
            }
        }
    }

    /// Stéatite : lentilles affleurantes de la lande d'altitude et de la haute montagne.
    fn soapstones(&self, reg: Region, probe: &SiteProbe, out: &mut Vec<Body>) {
        for (i, j) in cells_near(SOAP_PITCH, reg, 14.0) {
            if hash01(self.grid_seed, i, j, 61) >= 0.35 {
                continue;
            }
            let (cx, cz) = cell_center(self.grid_seed ^ 0x61, SOAP_PITCH, i, j);
            let m = (probe.macros)(cx, cz);
            let w = (probe.water)(cx, cz);
            if !m.is_land() || w.is_wet() || f64::from(w.top) <= altitudes(m.t0, self.sea).tree_line
            {
                continue;
            }
            let u = unit_xz(self.grid_seed, i, j, 62);
            let e0 = [u[0], 0.0, u[1]];
            let e2 = [-u[1], 0.0, u[0]];
            out.push(Body {
                kind: BodyKind::Soapstone,
                c: [
                    f64::from(cx) + 0.5,
                    f64::from(w.top) - 1.0,
                    f64::from(cz) + 0.5,
                ],
                e: [e0, [0.0, 1.0, 0.0], e2],
                a: [10.0, 4.0, 8.0],
                wobble: 0.25,
                reach: 14.0,
            });
        }
    }

    /// Grottes marines : demi-ellipsoïdes ouverts sur la mer au pied d'une falaise.
    fn sea_caves(&self, reg: Region, probe: &SiteProbe, out: &mut Vec<Body>) {
        const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for (i, j) in cells_near(SEA_CAVE_PITCH, reg, 14.0) {
            if hash01(self.grid_seed, i, j, 71) >= 0.4 {
                continue;
            }
            let (cx, cz) = cell_center(self.grid_seed ^ 0x71, SEA_CAVE_PITCH, i, j);
            let w = (probe.water)(cx, cz);
            if w.is_wet() || w.top < self.sea + 4 {
                continue;
            }
            let rock = self.province(cx, cz, &(probe.macros)(cx, cz)).0;
            let biome = (probe.biome)(cx, cz);
            let cliff = matches!(biome, NorrBiome::Klintkust | NorrBiome::Hogkust)
                || rock == Province::Limestone;
            if !cliff {
                continue;
            }
            // Mer à 3..12 blocs dans une des 4 directions.
            let mut found = None;
            'dirs: for (dx, dz) in DIRS {
                for k in 3..=12 {
                    let c = (probe.water)(cx + dx * k, cz + dz * k);
                    if c.is_wet() && c.kind == WaterKind::Sea {
                        found = Some((dx, dz, k));
                        break 'dirs;
                    }
                    if c.is_wet() {
                        break;
                    }
                }
            }
            let Some((dx, dz, k)) = found else { continue };
            // Axe long vers la mer : centre à 2 blocs derrière la ligne d'eau.
            let (ux, uz) = (f64::from(dx), f64::from(dz));
            let mouth = f64::from(k) - 1.0;
            let depth = 8.0 + 4.0 * hash01(self.grid_seed, i, j, 72);
            let c = [
                f64::from(cx) + 0.5 + ux * (mouth - depth * 0.4),
                f64::from(self.sea) + 1.0,
                f64::from(cz) + 0.5 + uz * (mouth - depth * 0.4),
            ];
            out.push(Body {
                kind: BodyKind::SeaCave,
                c,
                e: [[ux, 0.0, uz], [0.0, 1.0, 0.0], [-uz, 0.0, ux]],
                a: [depth, 4.5, 5.0 + 2.0 * hash01(self.grid_seed, i, j, 73)],
                wobble: 0.2,
                reach: depth + 2.0,
            });
        }
    }

    /// Salles de la montagne : grande salle ellipsoïdale sous la chaîne, une cellule sur quatre.
    fn halls(&self, reg: Region, probe: &SiteProbe, out: &mut Vec<Body>) {
        for (i, j) in cells_near(HALL_PITCH, reg, 16.0) {
            if hash01(self.grid_seed, i, j, 81) >= 0.25 {
                continue;
            }
            let (cx, cz) = cell_center(self.grid_seed ^ 0x81, HALL_PITCH, i, j);
            let m = (probe.macros)(cx, cz);
            let w = (probe.water)(cx, cz);
            if m.u < 0.55 || w.is_wet() {
                continue;
            }
            let cy = f64::from(w.top) - 32.0;
            if cy - 8.0 < 3.0 {
                continue;
            }
            let u = unit_xz(self.grid_seed, i, j, 82);
            out.push(Body {
                kind: BodyKind::Hall,
                c: [f64::from(cx) + 0.5, cy, f64::from(cz) + 0.5],
                e: [[u[0], 0.0, u[1]], [0.0, 1.0, 0.0], [-u[1], 0.0, u[0]]],
                a: [14.0, 7.0, 11.0],
                wobble: 0.15,
                reach: 16.0,
            });
        }
    }

    // -----------------------------------------------------------------------------------------
    // Cellule
    // -----------------------------------------------------------------------------------------

    /// Roche de socle de la cellule `y` : djupberg sous [`DEEP_Y`], craie de Klintkust, sinon la
    /// roche de la province.
    fn bulk(&self, c: &SubColumn, y: i32) -> BlockId {
        if y < DEEP_Y {
            self.blocks.deep
        } else if c.klint && y >= self.sea - CHALK_BELOW {
            self.blocks.chalk
        } else {
            c.rock
        }
    }

    /// Bloc de la cellule `(c.wx, y, c.wz)` sous la surface (`y < top`). `base` est le bloc que
    /// le terrain y poserait sans sous-sol ; `bulk` dit si la cellule est du socle (sinon, c'est
    /// la couverture meuble : seuls les affleurements l'écrasent).
    pub fn cell(&self, c: &SubColumn, sites: &Sites, y: i32, base: BlockId, bulk: bool) -> BlockId {
        let b = &self.blocks;
        if y <= 1 {
            return b.bedrock;
        }
        let (wx, wz) = (c.wx, c.wz);
        let p = [f64::from(wx) + 0.5, f64::from(y) + 0.5, f64::from(wz) + 0.5];
        let mut id = if bulk { self.bulk(c, y) } else { base };

        // Corps de la grille : affleurements (écrasent la couverture) puis cavités.
        for body in &sites.bodies {
            let Some(s) = body.value(p) else { continue };
            if s >= 1.0 + body.wobble {
                continue;
            }
            let lim = 1.0 + body.wobble * self.wobble.n(p[0] / 12.0, p[1] / 12.0, p[2] / 12.0);
            if s >= lim {
                continue;
            }
            match body.kind {
                BodyKind::Malmberg => {
                    id = if s < 0.55 { b.rich_iron } else { b.iron };
                }
                BodyKind::Soapstone => id = b.soapstone,
                BodyKind::Copper if bulk => {
                    id = if s < 0.56 {
                        b.copper
                    } else if self.flint.n(p[0] / 3.0, p[1] / 3.0, p[2] / 3.0) < 0.5 {
                        b.red_earth
                    } else {
                        id
                    };
                }
                BodyKind::Cobalt if bulk => id = b.cobalt,
                BodyKind::Tungsten if bulk => id = b.tungsten,
                BodyKind::SeaCave => {
                    if y >= self.sea - 2 && y <= c.top - 3 && c.coast_ok && !c.wet {
                        return self.carved(y);
                    }
                }
                BodyKind::Hall if y <= c.ceiling => return self.carved(y),
                _ => {}
            }
        }
        // Cavités ouvertes sur le ciel ou noyées (failles, marmites, effondrements) : elles
        // traversent aussi la couverture meuble.
        if let Some(open) = self.open_cut(c, y) {
            return open;
        }
        if !bulk {
            return self.refine_cover(c, y, id);
        }

        // Filons de diabase : murs verticaux qui traversent tout le socle.
        if c.dike {
            id = b.diabase;
        }
        // Silex : bandes horizontales dans la craie.
        if c.klint && id == b.chalk {
            let band = (y + (self.flint.n(p[0] / 25.0, 0.5, p[2] / 25.0) * 4.0) as i32) % 6;
            if band == 0 && self.flint.n(p[0] / 4.0, p[1] / 4.0, p[2] / 4.0) > 0.55 {
                id = b.flint;
            }
        }
        // Plaques au ras de la couverture.
        let under = c.cover_bottom - y;
        if c.bog_ore && (1..=2).contains(&under) {
            id = b.bog_ore;
        }
        // Filons.
        if !c.dike && y >= 2 {
            id = self.veins(c, y, p, id);
        }

        // Cavités enterrées.
        if self.carve(c, y, p) {
            return self.carved(y);
        }
        id
    }

    /// Couverture meuble : paillettes d'or des rivières, minerai des lacs, argile et sable.
    fn refine_cover(&self, c: &SubColumn, y: i32, id: BlockId) -> BlockId {
        let b = &self.blocks;
        if c.gold_placer && y == c.top - 1 {
            return b.gold_gravel;
        }
        if c.lake_ore && y >= c.top - 2 {
            return b.lake_ore;
        }
        if let Some(blk) = c.under_plaque {
            if y == c.top - 3 || y == c.top - 2 {
                return blk;
            }
        }
        if let Some(blk) = c.plaque {
            if y == c.top - 1 {
                return blk;
            }
        }
        if c.cap && y == c.top - 1 {
            return b.red_earth;
        }
        if c.bog_ore && c.cover_bottom - y == 1 {
            return b.bog_ore;
        }
        id
    }

    /// Filons de nappes de bruit 3D (fer de roche, argent, quartz et or du nord).
    fn veins(&self, c: &SubColumn, y: i32, p: [f64; 3], id: BlockId) -> BlockId {
        let b = &self.blocks;
        if c.mining && y >= IRON_Y.0 && y <= IRON_Y.1.min(c.top - 2) {
            let v = self.iron_a.sheet(p[0] / 40.0, p[1] / 40.0, p[2] / 40.0);
            if v < IRON_WIDTH && self.iron_b.n(p[0] / 90.0, p[1] / 90.0, p[2] / 90.0) > 0.55 {
                return if v < IRON_RICH { b.rich_iron } else { b.iron };
            }
        }
        if c.province == Province::Limestone
            && y >= SILVER_Y.0
            && y <= SILVER_Y.1
            && self.silver.sheet(p[0] / 28.0, p[1] / 28.0, p[2] / 28.0) < SILVER_WIDTH
        {
            return b.silver;
        }
        if c.north
            && y <= GOLD_Y_MAX
            && self.quartz.sheet(p[0] / 30.0, p[1] / 30.0, p[2] / 30.0) < GOLD_WIDTH
        {
            let rich = self.gold.n(p[0] / 9.0, p[1] / 9.0, p[2] / 9.0) > 0.78;
            return if rich { b.gold_quartz } else { b.quartz };
        }
        id
    }

    /// Cavités qui s'ouvrent jusqu'au sol : faille, marmite de géant (noyée dans un lit de
    /// rivière : jamais d'air sous l'eau), effondrement de tube de lave. `None` si la cellule
    /// n'en fait pas partie.
    fn open_cut(&self, c: &SubColumn, y: i32) -> Option<BlockId> {
        if c.fault > 0 && y >= c.top - c.fault && y >= 4 {
            return Some(self.carved(y));
        }
        if c.pothole > 0 && y >= c.top - c.pothole && y >= 4 {
            return Some(if c.wet {
                self.blocks.water
            } else {
                self.carved(y)
            });
        }
        if c.lava_shaft {
            if let Some(yc) = c.lava_tube {
                if y >= yc - 2 {
                    return Some(self.carved(y));
                }
            }
        }
        None
    }

    /// Vrai si une cavité enterrée (tube de lave, karst) creuse la cellule de socle : elle garde
    /// [`ROOF`] blocs de roche au-dessus d'elle et sous les colonnes voisines.
    fn carve(&self, c: &SubColumn, y: i32, p: [f64; 3]) -> bool {
        if y > c.ceiling {
            return false;
        }
        if let Some(yc) = c.lava_tube {
            if (y - yc).abs() <= 2 {
                return true;
            }
        }
        // Karst : intersection de deux nappes de bruit 3D (tubes).
        if c.karst && y >= KARST_Y_MIN {
            let a = self.karst_a.sheet(
                p[0] / KARST_SCALE[0],
                p[1] / KARST_SCALE[1],
                p[2] / KARST_SCALE[2],
            );
            if a < KARST_WIDTH {
                let bb = self.karst_b.sheet(
                    p[0] / KARST_SCALE[0],
                    p[1] / KARST_SCALE[1],
                    p[2] / KARST_SCALE[2],
                );
                if bb < KARST_WIDTH {
                    return true;
                }
            }
        }
        false
    }

    /// Cellule creusée : noyée sous le niveau de la mer, d'air au-dessus (règle d'étanchéité
    /// unique des cavités).
    fn carved(&self, y: i32) -> BlockId {
        if y < self.sea {
            self.blocks.water
        } else {
            0
        }
    }

    /// Ouverture d'un tunnel de fonte dans la glace d'un glacier : la colonne est une galerie, de
    /// `surface` à `surface + 2`, si la glace y fait au moins 6 blocs.
    pub fn ice_tunnel(&self, c: &SubColumn, surface: i32, ice_top: i32, y: i32) -> bool {
        c.ice_tunnel && ice_top - surface >= 6 && y < surface + 2
    }

    /// Chapeau de fer : un filon de fer passe à 5 blocs sous la surface de la colonne sèche ;
    /// la terre rouille se lit en surface.
    fn iron_cap(&self, wx: i32, wz: i32, top: i32) -> bool {
        let y = top - 5;
        if y < IRON_Y.0 || y > IRON_Y.1 {
            return false;
        }
        let (x, yy, z) = (f64::from(wx) + 0.5, f64::from(y) + 0.5, f64::from(wz) + 0.5);
        self.iron_a.sheet(x / 40.0, yy / 40.0, z / 40.0) < IRON_WIDTH * 1.4
            && self.iron_b.n(x / 90.0, yy / 90.0, z / 90.0) > 0.55
    }
}
