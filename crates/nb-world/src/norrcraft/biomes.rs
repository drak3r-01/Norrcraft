//! Biomes Norrcraft (session 2.1 ; GDD doc 02 §7, doc 04 §9).
//!
//! * [`NorrBiome`] : les 25 biomes du doc 02 §7.3 (24 de surface et l'Älv, linéaire). Leur
//!   identifiant moteur ([`BiomeId`]) est leur rang dans l'enum, qui est aussi leur rang dans
//!   `assets/norrcraft/biomes.json` (teintes, ambiances, sols).
//! * [`BiomeSelector`] : règles de sélection du doc 02 §7.2 (premier test vrai), sur l'étagement
//!   du §7.1, avec les **frontières tramées** du §7.4 : chaque comparaison à un seuil d'un champ
//!   continu est tirée avec la probabilité `sm(−12, 12, d)`, où `d = (valeur − seuil) / |∇valeur|`
//!   est la distance signée estimée à la frontière, en blocs, comparée au hachage `h(graine, x, z)`
//!   de la colonne. Les tests de formes physiques (lave, glace, mer, rivière, type de côte,
//!   esker) restent francs : le sol doit coïncider avec la forme.
//!
//! Le relief ne dépend d'aucun biome (doc 02 §7.4, point 1) : la sélection ne fait que lire le
//! relief, les formes et l'eau déjà calculés.

use std::sync::OnceLock;

use nb_data::universe::NORRCRAFT_BIOMES_JSON;
use nb_data::{BlockId, BlockRegistry};
use serde::Deserialize;

use super::climate::altitudes;
use super::fields::{MacroFields, MacroSample, Province};
use super::height::Relief;
use super::hydro::{WaterColumn, WaterKind};
use super::landforms::Coast;
use super::math::{hash01, smooth, spread, GradField};
use crate::profile::{BiomeAmbience, BiomeId, BiomeInfo, Precipitation};

/// Constante de semence du tramage (`graine XOR constante`) ; `0x…04xx` = biomes.
const DITHER_SEED: u64 = 0x4E52_5743_0000_0401;
/// Constante de semence des taches de sol (`patch` de `biomes.json`).
const PATCH_SEED: u64 = 0x4E52_5743_0000_0402;

/// Demi-largeur de la frontière tramée, en blocs (doc 02 §7.4 : `sm(−12, 12, d)`).
pub const DITHER_BAND: f64 = 12.0;
/// Largeur des berges rattachées à l'Älv de part et d'autre du lit, en blocs.
pub const RIVER_BANK: f64 = 4.0;
/// Pente (norme du gradient de la hauteur, en blocs par bloc) sous laquelle un sol est « plat »
/// (Palsmyr).
pub const FLAT_GRADIENT: f64 = 0.6;
/// Bergslag : seulement au-dessus de `S + 3` (relief moyen ; les terres plus basses sont des rives).
pub const BERGSLAG_MIN_ABOVE_SEA: f64 = 3.0;

// Seuils réglés sur les cibles du doc 02 §7.5 (écarts au §7.2 consignés dans
// docs/WORLDGEN_NORRCRAFT.md) : les champs de la session 1.1 donnent près de l'origine une zone
// hémiboréale bien plus vaste (44 % des terres) que ce que les cibles supposent.

/// Myr : masque de tourbière `My` au-delà duquel la colonne est une tourbière (doc : 0,5, qui ne
/// garde que le cœur de tourbe : 0,5 % de la carte au lieu de 6).
pub const MYR_MASK: f64 = 0.15;
/// Brännfält : `Vr` au-delà duquel une forêt boréale a brûlé (doc : 0,92, que `Vr` n'atteint
/// presque jamais : son 95ᵉ centile est 0,81).
pub const BRANNFALT_VR: f64 = 0.78;
/// Zone hémiboréale (ajout) : `Hm` au-delà duquel la forêt mixte (Urskog) remplace le bocage.
pub const HEMIBOREAL_FOREST_HM: f64 = 0.50;
/// Zone hémiboréale (ajout) : `Hm` en deçà duquel la pinède sur sable (Tallhed) remplace le
/// bocage.
pub const HEMIBOREAL_HEATH_HM: f64 = 0.35;
/// Bokskog : `Hm` au-delà duquel la zone némorale porte une hêtraie (doc : 0,45).
pub const BOKSKOG_HM: f64 = 0.58;
/// Alvar : érosion `E` au-delà de laquelle le calcaire némoral est une dalle (doc : 0,6).
pub const ALVAR_E: f64 = 0.35;
/// Myr (ajout) : hors de la tourbière physique, une plaine boréale ou hémiboréale plate d'humidité
/// `Hm` au-delà de ce seuil est aussi une tourbière (le masque `My` de la 1.3 est trop étroit).
pub const MYR_WET_HM: f64 = 0.78;
/// Provinces rocheuses (Bergslag, Alvar) : la colonne est dans la province si son poids de
/// frontière dépasse `PROVINCE_DITHER · h(graine, x, z)` (tramage sur ~30 blocs au lieu de 60).
pub const PROVINCE_DITHER: f64 = 0.5;
/// Subarctique de basse altitude : `Vr` au-delà duquel la boulaie l'emporte sur la lande.
pub const SUBARCTIC_BIRCH_VR: f64 = 0.72;

/// Les biomes de Norrcraft (doc 02 §7.3), dans l'ordre des identifiants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum NorrBiome {
    Vasterhav,
    Djuphav,
    Bottenhav,
    Packis,
    Fjord,
    Skargard,
    Klintkust,
    Hogkust,
    Strandang,
    Bokskog,
    Hagmark,
    Alvar,
    Sjoland,
    Urskog,
    Tallhed,
    Myr,
    Alv,
    Brannfalt,
    Bergslag,
    Fjallbjorkskog,
    Fjallhed,
    Kalfjall,
    Glaciar,
    Palsmyr,
    Lavafalt,
}

impl NorrBiome {
    pub const COUNT: usize = 25;
    pub const ALL: [NorrBiome; Self::COUNT] = [
        Self::Vasterhav,
        Self::Djuphav,
        Self::Bottenhav,
        Self::Packis,
        Self::Fjord,
        Self::Skargard,
        Self::Klintkust,
        Self::Hogkust,
        Self::Strandang,
        Self::Bokskog,
        Self::Hagmark,
        Self::Alvar,
        Self::Sjoland,
        Self::Urskog,
        Self::Tallhed,
        Self::Myr,
        Self::Alv,
        Self::Brannfalt,
        Self::Bergslag,
        Self::Fjallbjorkskog,
        Self::Fjallhed,
        Self::Kalfjall,
        Self::Glaciar,
        Self::Palsmyr,
        Self::Lavafalt,
    ];

    /// Identifiant moteur (indice dans [`crate::WorldProfile::biomes`]).
    #[inline]
    pub fn id(self) -> BiomeId {
        self as BiomeId
    }

    pub fn from_id(id: BiomeId) -> Option<Self> {
        Self::ALL.get(usize::from(id)).copied()
    }

    /// Nom stable (identifiant de données, suédois sans diacritiques, en capitales).
    pub fn name(self) -> &'static str {
        match self {
            Self::Vasterhav => "VASTERHAV",
            Self::Djuphav => "DJUPHAV",
            Self::Bottenhav => "BOTTENHAV",
            Self::Packis => "PACKIS",
            Self::Fjord => "FJORD",
            Self::Skargard => "SKARGARD",
            Self::Klintkust => "KLINTKUST",
            Self::Hogkust => "HOGKUST",
            Self::Strandang => "STRANDANG",
            Self::Bokskog => "BOKSKOG",
            Self::Hagmark => "HAGMARK",
            Self::Alvar => "ALVAR",
            Self::Sjoland => "SJOLAND",
            Self::Urskog => "URSKOG",
            Self::Tallhed => "TALLHED",
            Self::Myr => "MYR",
            Self::Alv => "ALV",
            Self::Brannfalt => "BRANNFALT",
            Self::Bergslag => "BERGSLAG",
            Self::Fjallbjorkskog => "FJALLBJORKSKOG",
            Self::Fjallhed => "FJALLHED",
            Self::Kalfjall => "KALFJALL",
            Self::Glaciar => "GLACIAR",
            Self::Palsmyr => "PALSMYR",
            Self::Lavafalt => "LAVAFALT",
        }
    }

    /// Biome de mer (cibles du doc 02 §7.5 : « mers », Skärgård compris).
    pub fn is_sea(self) -> bool {
        matches!(
            self,
            Self::Vasterhav
                | Self::Djuphav
                | Self::Bottenhav
                | Self::Packis
                | Self::Fjord
                | Self::Skargard
        )
    }

    /// Biome de côte (cible « côtes » du doc 02 §7.5).
    pub fn is_coast(self) -> bool {
        matches!(self, Self::Klintkust | Self::Hogkust | Self::Strandang)
    }
}

/// Cibles de répartition du doc 02 §7.5 (préréglage Norrland, 16 × 16 km autour de l'origine), en
/// pour cent de la carte : (catégorie, biomes comptés, part visée).
pub const DISTRIBUTION_TARGETS: &[(&str, &[NorrBiome], f64)] = &[
    (
        "mers ouvertes",
        &[
            NorrBiome::Vasterhav,
            NorrBiome::Djuphav,
            NorrBiome::Bottenhav,
        ],
        21.0,
    ),
    ("Fjord", &[NorrBiome::Fjord], 2.0),
    ("Skärgård", &[NorrBiome::Skargard], 4.0),
    ("Packis", &[NorrBiome::Packis], 3.0),
    ("Urskog", &[NorrBiome::Urskog], 14.0),
    ("Tallhed", &[NorrBiome::Tallhed], 7.0),
    ("Myr", &[NorrBiome::Myr], 6.0),
    ("Sjöland", &[NorrBiome::Sjoland], 7.0),
    ("Hagmark", &[NorrBiome::Hagmark], 6.0),
    ("Bokskog", &[NorrBiome::Bokskog], 3.0),
    ("Alvar", &[NorrBiome::Alvar], 1.0),
    ("Fjällbjörkskog", &[NorrBiome::Fjallbjorkskog], 5.0),
    ("Fjällhed", &[NorrBiome::Fjallhed], 6.0),
    ("Kalfjäll", &[NorrBiome::Kalfjall], 3.0),
    ("Glaciär", &[NorrBiome::Glaciar], 1.0),
    ("Palsmyr", &[NorrBiome::Palsmyr], 2.0),
    ("Bergslag", &[NorrBiome::Bergslag], 2.0),
    (
        "côtes",
        &[
            NorrBiome::Klintkust,
            NorrBiome::Hogkust,
            NorrBiome::Strandang,
        ],
        4.0,
    ),
    ("Brännfält", &[NorrBiome::Brannfalt], 0.5),
    ("Lavafält", &[NorrBiome::Lavafalt], 0.5),
    ("Älv", &[NorrBiome::Alv], 2.0),
];

// ---------------------------------------------------------------------------------------------
// Données (assets/norrcraft/biomes.json)
// ---------------------------------------------------------------------------------------------

/// Tache de sol : `share` des colonnes plates reçoivent `block` en surface.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PatchDef {
    pub block: String,
    pub share: f64,
}

/// Sol d'un biome, par noms de blocs.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SoilDef {
    /// Peau d'un bloc sur les colonnes sèches et plates (`""` : aucune).
    pub top: String,
    /// Couverture meuble sous la peau.
    pub filler: String,
    /// Épaisseur de la couverture meuble.
    pub depth: i32,
    /// Roche visible sur les pentes raides.
    pub rock: String,
    /// Fond sous l'eau.
    pub floor: String,
    #[serde(default)]
    pub patch: Option<PatchDef>,
}

/// Entrée de `biomes.json`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BiomeDef {
    pub name: String,
    /// Glose française.
    pub gloss: String,
    pub grass_tint: [f64; 3],
    pub leaves_tint: [f64; 3],
    pub water_color: [f64; 3],
    pub temperature: f32,
    pub precipitation: String,
    pub music: String,
    /// `[vent, oiseaux, grillons, grenouilles, eau, lave]`.
    pub ambience: [f32; 6],
    pub soil: SoilDef,
}

/// Table des biomes lue une fois dans `assets/norrcraft/biomes.json` (embarqué).
///
/// Panique si le fichier ne décrit pas exactement les biomes de [`NorrBiome`], dans l'ordre.
pub fn biome_defs() -> &'static [BiomeDef] {
    static DEFS: OnceLock<Vec<BiomeDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let defs: Vec<BiomeDef> =
            serde_json::from_str(NORRCRAFT_BIOMES_JSON).expect("assets/norrcraft/biomes.json");
        assert_eq!(defs.len(), NorrBiome::COUNT, "biomes.json : 25 biomes");
        for (d, b) in defs.iter().zip(NorrBiome::ALL) {
            assert_eq!(d.name, b.name(), "biomes.json : ordre des biomes");
        }
        defs
    })
}

/// Table `BiomeId → BiomeInfo` du profil Norrcraft.
pub fn biome_infos() -> Vec<BiomeInfo> {
    biome_defs()
        .iter()
        .map(|d| BiomeInfo {
            name: d.name.clone(),
            grass_tint: d.grass_tint,
            leaves_tint: d.leaves_tint,
            water_color: d.water_color,
            temperature: d.temperature,
            precipitation: match d.precipitation.as_str() {
                "snow" => Precipitation::Snow,
                "none" => Precipitation::None,
                _ => Precipitation::Rain,
            },
            music: d.music.clone(),
            ambience: BiomeAmbience {
                wind: d.ambience[0],
                birds: d.ambience[1],
                crickets: d.ambience[2],
                frogs: d.ambience[3],
                water: d.ambience[4],
                lava: d.ambience[5],
            },
        })
        .collect()
}

/// Sol d'un biome résolu dans le registre du monde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiomeSoil {
    pub top: Option<BlockId>,
    pub filler: BlockId,
    pub depth: i32,
    pub rock: BlockId,
    pub floor: BlockId,
    /// Bloc de la tache et part en millièmes.
    pub patch: Option<(BlockId, u16)>,
}

impl BiomeSoil {
    /// Sols de tous les biomes. Panique sur un nom de bloc inconnu de l'univers.
    pub fn table(registry: &BlockRegistry) -> [BiomeSoil; NorrBiome::COUNT] {
        let id = |name: &str| {
            let b = registry.get(name);
            assert!(
                b.name == name,
                "biomes.json : bloc « {name} » absent de Norrcraft"
            );
            b.id
        };
        let defs = biome_defs();
        std::array::from_fn(|i| {
            let s = &defs[i].soil;
            BiomeSoil {
                top: (!s.top.is_empty()).then(|| id(&s.top)),
                filler: id(&s.filler),
                depth: s.depth,
                rock: id(&s.rock),
                floor: id(&s.floor),
                patch: s
                    .patch
                    .as_ref()
                    .map(|p| (id(&p.block), (p.share.clamp(0.0, 1.0) * 1000.0) as u16)),
            }
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Sélection
// ---------------------------------------------------------------------------------------------

/// Une colonne et ses 4 voisines : indices 0 = centre, 1 = −x, 2 = +x, 3 = −z, 4 = +z. Les
/// voisines servent aux gradients (différences centrées) des frontières tramées.
#[derive(Debug, Clone, Copy)]
pub struct Neighbourhood<'a> {
    pub m: [&'a MacroSample; 5],
    pub r: [&'a Relief; 5],
}

/// Décalages des 5 colonnes d'un [`Neighbourhood`].
pub const NEIGHBOURHOOD: [(i32, i32); 5] = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)];

/// Résultat de la sélection pour une colonne.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Selection {
    pub biome: NorrBiome,
    /// Lac ou rivière : eau douce (doc 02 §7.2, règle 4).
    pub fresh_water: bool,
    /// Au-dessus des neiges éternelles `y_n` (tramé) et au sec : neige en surface.
    pub snow: bool,
    /// Tirage de tramage `h(graine, x, z)` de la colonne, dans [0, 1).
    pub dither: f64,
}

/// Sélection des biomes (doc 02 §7.2), fonction pure de (graine, colonne).
#[derive(Debug, Clone)]
pub struct BiomeSelector {
    dither_seed: u64,
    patch_seed: u64,
    patches: GradField,
    sea: i32,
}

/// Longueur d'onde des taches de sol, en blocs (taches de quelques blocs, bords tramés).
const PATCH_WAVELENGTH: f64 = 6.0;

/// Zones de température tramées (0 = arctique … 4 = némorale).
const ARCTIC: usize = 0;
const SUBARCTIC: usize = 1;
const BOREAL: usize = 2;
const HEMIBOREAL: usize = 3;
const ZONE_THRESHOLDS: [f64; 4] = [0.12, 0.28, 0.50, 0.70];

impl BiomeSelector {
    pub fn new(seed: i64, sea_level: i32) -> Self {
        Self {
            dither_seed: (seed as u64) ^ DITHER_SEED,
            patch_seed: (seed as u64) ^ PATCH_SEED,
            patches: GradField::new(seed, PATCH_SEED),
            sea: sea_level,
        }
    }

    /// Tirage des taches de sol de la colonne, en millièmes : bruit étalé (taches groupées de
    /// quelques blocs) mêlé d'un quart de hachage (bords irréguliers). Une tache de part `p` couvre
    /// les colonnes de tirage inférieur à `p`.
    pub fn patch_draw(&self, wx: i32, wz: i32) -> u16 {
        let (x, z) = (f64::from(wx), f64::from(wz));
        let n = spread(
            self.patches
                .fbm(x / PATCH_WAVELENGTH, z / PATCH_WAVELENGTH, 2),
        );
        let v = 0.75 * n + 0.25 * hash01(self.patch_seed, wx, 0, wz);
        (v * 1000.0) as u16
    }

    /// Biome de la colonne `(wx, wz)` : champs et relief de la colonne et de ses voisines `nb`,
    /// colonne d'eau scellée `w`. `fields` sert aux provinces (lues seulement si une règle en a
    /// besoin).
    pub fn select(
        &self,
        wx: i32,
        wz: i32,
        nb: &Neighbourhood,
        w: &WaterColumn,
        fields: &MacroFields,
    ) -> Selection {
        let u = hash01(self.dither_seed, wx, 0, wz);
        let m = nb.m[0];
        let r = nb.r[0];
        let f = &r.forms;
        let sea = f64::from(self.sea);

        // Norme du gradient (différences centrées) d'une grandeur de la colonne.
        let grad = |v: &dyn Fn(&MacroSample, &Relief) -> f64| {
            let gx = (v(nb.m[2], nb.r[2]) - v(nb.m[1], nb.r[1])) * 0.5;
            let gz = (v(nb.m[4], nb.r[4]) - v(nb.m[3], nb.r[3])) * 0.5;
            (gx * gx + gz * gz).sqrt()
        };
        // Comparaisons tramées : vraie avec la probabilité sm(−12, 12, d).
        let above =
            |v: f64, t: f64, g: f64| smooth(-DITHER_BAND, DITHER_BAND, (v - t) / g.max(1e-9)) > u;
        let below = |v: f64, t: f64, g: f64| above(-v, -t, g);

        let wet = w.is_wet();
        let fresh_water = wet && matches!(w.kind, WaterKind::Lake | WaterKind::River);
        let g_h = grad(&|_, r| r.h);
        // Altitude : le sol, ou la surface d'un lac.
        let y = if wet { f64::from(w.level) } else { r.h };
        let alt = altitudes(m.t0, self.sea);
        let snow = !wet && above(y, alt.snow_line, g_h);
        let done = |biome| Selection {
            biome,
            fresh_water,
            snow,
            dither: u,
        };

        let g_t0 = grad(&|m, _| m.t0);
        let zone = ZONE_THRESHOLDS
            .iter()
            .filter(|&&t| above(m.t0, t, g_t0))
            .count();

        // 1. Champ de lave (le basalte des formes : poids de lave > 0,5).
        if f.lava > 0.5 {
            return done(NorrBiome::Lavafalt);
        }
        // 2. Glacier.
        if f.ice > 0.5 {
            return done(NorrBiome::Glaciar);
        }
        // 3. Mer.
        if wet && w.kind == WaterKind::Sea {
            let b = if zone == ARCTIC {
                NorrBiome::Packis
            } else if r.fjord {
                NorrBiome::Fjord
            } else if above(f.archipelago, 0.3, grad(&|_, r| r.forms.archipelago)) {
                NorrBiome::Skargard
            } else if below(m.o, 0.5, grad(&|m, _| m.o)) {
                NorrBiome::Bottenhav
            } else if below(m.c, 0.25, grad(&|m, _| m.c)) {
                NorrBiome::Djuphav
            } else {
                NorrBiome::Vasterhav
            };
            return done(b);
        }
        // 4. Rivière et ses berges ; un lac prend le biome environnant (eau douce).
        if (wet && w.kind == WaterKind::River)
            || (!wet && w.channel_distance < w.river_width + RIVER_BANK)
        {
            return done(NorrBiome::Alv);
        }
        // 5. Étagement.
        if above(y, alt.bare_rock, g_h) {
            return done(NorrBiome::Kalfjall);
        }
        if above(y, alt.tree_line, g_h) {
            return done(NorrBiome::Fjallhed);
        }
        if above(y, alt.birch_belt, g_h) {
            return done(NorrBiome::Fjallbjorkskog);
        }
        // 6. Subarctique et arctique de basse altitude.
        if zone <= SUBARCTIC {
            let flat = g_h < FLAT_GRADIENT;
            let wetland = f.palsa > 0.0 || f.mire > 0.5 || above(m.hm, 0.6, grad(&|m, _| m.hm));
            let b = if flat && wetland {
                NorrBiome::Palsmyr
            } else if above(m.vr, SUBARCTIC_BIRCH_VR, grad(&|m, _| m.vr)) {
                NorrBiome::Fjallbjorkskog
            } else {
                NorrBiome::Fjallhed
            };
            return done(b);
        }
        // 7. Côtes : les formes de côte de la 1.3 sont côtières par construction (leur emprise
        // déborde C < 0,50) ; les dunes portent la pinède sur sable.
        let coast = match f.coast {
            Coast::Klint => Some(NorrBiome::Klintkust),
            Coast::Raised => Some(NorrBiome::Hogkust),
            Coast::Skargard => Some(NorrBiome::Skargard),
            Coast::SaltMeadow => Some(NorrBiome::Strandang),
            Coast::Dunes => Some(NorrBiome::Tallhed),
            Coast::None => None,
        };
        if let Some(b) = coast {
            return done(b);
        }
        if below(m.c, 0.50, grad(&|m, _| m.c))
            && above(f.archipelago, 0.3, grad(&|_, r| r.forms.archipelago))
        {
            return done(NorrBiome::Skargard);
        }
        // 8. Tourbière.
        if above(f.mire, MYR_MASK, grad(&|_, r| r.forms.mire))
            || (zone <= HEMIBOREAL
                && g_h < FLAT_GRADIENT
                && above(m.hm, MYR_WET_HM, grad(&|m, _| m.hm)))
        {
            return done(NorrBiome::Myr);
        }
        // Province rocheuse, lue au plus une fois (poids nul à la frontière : tramage sur ~60 blocs).
        let mut province = None;
        let mut in_province = |p: Province| {
            let (here, weight) = *province
                .get_or_insert_with(|| fields.province_blend(f64::from(wx), f64::from(wz)));
            here == p && weight > PROVINCE_DITHER * u
        };
        // 9. Pays minier.
        if y > sea + BERGSLAG_MIN_ABOVE_SEA && in_province(Province::Greenstone) {
            return done(NorrBiome::Bergslag);
        }
        let b = match zone {
            // 10. Boréale.
            BOREAL => {
                if f.esker > 1.0 || below(m.hm, 0.35, grad(&|m, _| m.hm)) {
                    NorrBiome::Tallhed
                } else if above(m.vr, BRANNFALT_VR, grad(&|m, _| m.vr)) {
                    NorrBiome::Brannfalt
                } else {
                    NorrBiome::Urskog
                }
            }
            // 11. Hémiboréale.
            HEMIBOREAL => {
                if above(m.lk, 0.6, grad(&|m, _| m.lk)) {
                    NorrBiome::Sjoland
                } else if above(m.hm, HEMIBOREAL_FOREST_HM, grad(&|m, _| m.hm)) {
                    NorrBiome::Urskog
                } else if below(m.hm, HEMIBOREAL_HEATH_HM, grad(&|m, _| m.hm)) {
                    NorrBiome::Tallhed
                } else {
                    NorrBiome::Hagmark
                }
            }
            // 12. Némorale.
            _ => {
                if in_province(Province::Limestone) && above(m.e, ALVAR_E, grad(&|m, _| m.e)) {
                    NorrBiome::Alvar
                } else if above(m.hm, BOKSKOG_HM, grad(&|m, _| m.hm)) {
                    NorrBiome::Bokskog
                } else {
                    NorrBiome::Hagmark
                }
            }
        };
        done(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_enum() {
        let defs = biome_defs();
        assert_eq!(defs.len(), NorrBiome::COUNT);
        for b in NorrBiome::ALL {
            assert_eq!(NorrBiome::from_id(b.id()), Some(b));
            assert_eq!(defs[usize::from(b.id())].name, b.name());
        }
        assert_eq!(NorrBiome::from_id(NorrBiome::COUNT as BiomeId), None);
        assert_eq!(biome_infos().len(), NorrBiome::COUNT);
    }

    #[test]
    fn soils_resolve_in_the_norrcraft_registry() {
        let reg = BlockRegistry::for_kind(nb_data::WorldKind::Norrcraft);
        let soils = BiomeSoil::table(&reg);
        let gras = reg.get("gras").id;
        assert_eq!(soils[NorrBiome::Hagmark as usize].top, Some(gras));
        assert_eq!(soils[NorrBiome::Vasterhav as usize].top, None);
        assert!(soils
            .iter()
            .all(|s| s.filler != 0 && s.rock != 0 && s.floor != 0));
    }
}
