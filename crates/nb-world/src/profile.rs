//! Profil d'univers (plan Norrcraft, session 0.2 ; GDD doc 01 §3).
//!
//! Le moteur ne connaît ni biome, ni mob, ni structure d'un univers : il parle à un
//! [`TerrainGenerator`] et lit une table [`BiomeInfo`] dans un [`WorldProfile`]. Chaque univers
//! (`vanilla`, `norrcraft`) fournit son propre profil.

use std::fmt;
use std::sync::Arc;

use crate::chunk::Chunk;
use crate::world::find_spawn_location;

/// Identifiant de biome vu par le moteur (indice dans [`WorldProfile::biomes`]).
///
/// Même type que `nb_mesh::BiomeId`. Pour vanilla, c'est l'ordinal de l'enum interne.
pub type BiomeId = u8;

/// Univers de contenu : défini dans `nb-data` (qui range les données par univers), réexporté ici.
pub use nb_data::WorldKind;

/// Précipitations d'un biome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precipitation {
    None,
    #[default]
    Rain,
    Snow,
}

/// Intensités sonores d'un biome, de 0 à 1 (hors grotte et eau libre).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BiomeAmbience {
    pub wind: f32,
    pub birds: f32,
    pub crickets: f32,
    pub frogs: f32,
    pub water: f32,
    pub lava: f32,
}

/// Description d'un biome pour le moteur : tout ce que le maillage, l'audio, l'apparition des
/// mobs et la survie ont besoin de savoir, sans connaître le type de biome de l'univers.
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeInfo {
    /// Nom stable (ex. `SNOWY_PLAINS`) ; c'est la clé des fichiers de données
    /// (`spawnBiomes` des mobs, `biomes` de la survie).
    pub name: String,
    pub grass_tint: [f64; 3],
    pub leaves_tint: [f64; 3],
    pub water_color: [f64; 3],
    /// Température indicative de 0 (glacial) à 1 (torride), échelle du générateur.
    pub temperature: f32,
    pub precipitation: Precipitation,
    /// Thème musical de surface, par clé (`meadow`, `forest`, `snow`…).
    pub music: String,
    pub ambience: BiomeAmbience,
}

impl BiomeInfo {
    /// Biome neutre portant seulement un nom (tests, mondes sans biomes).
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            grass_tint: [0.3, 0.5, 0.2],
            leaves_tint: [0.3, 0.5, 0.2],
            water_color: [0.2, 0.4, 0.8],
            temperature: 0.5,
            precipitation: Precipitation::Rain,
            music: "meadow".to_owned(),
            ambience: BiomeAmbience::default(),
        }
    }
}

/// Climat d'une colonne, pour les univers qui en ont un (saisons, chaleur, durée du jour).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Climate {
    /// Température brute du générateur (échelle propre à l'univers).
    pub temperature: f64,
    pub humidity: f64,
    /// Latitude λ de 0 (sud, tempéré) à 1 (pôle), si l'univers en a une. Norrcraft : λ = 1 − T₀ ;
    /// la durée du jour en dépend (`crate::calendar`).
    pub latitude: Option<f64>,
    /// Océanité O (0 baltique, 1 atlantique) ; 0,5 sans océan.
    pub ocean: f64,
    /// Continentalité C (0 haute mer, 0,465 trait de côte, 1 intérieur) ; 0,5 sans océan.
    pub continental: f64,
    /// Relief accidenté (1 − E) : fjords et vallées.
    pub rugged: f64,
    /// Axe des fjords (vecteur unitaire x, z).
    pub grain: [f64; 2],
    /// Niveau de la mer propre au générateur.
    pub sea_level: i32,
}

/// Interface unique, pure et réentrante, par laquelle le moteur parle à un générateur.
pub trait TerrainGenerator: Send + Sync + fmt::Debug {
    /// Copie indépendante au résultat identique : une instance par thread de génération.
    fn fork(&self) -> Box<dyn TerrainGenerator>;

    /// Premier `y` libre de la colonne `(wx, wz)`.
    fn surface_height(&self, wx: i32, wz: i32) -> i32;

    /// Biome de la colonne (indice dans [`WorldProfile::biomes`]).
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId;

    /// Remplit un chunk supposé vide. Les drapeaux sont à la charge de l'appelant.
    fn generate_chunk(&self, chunk: &mut Chunk);

    /// Position de départ `[x, y, z]` (pieds), cherchée en anneaux autour de l'origine.
    fn find_spawn(&self, sea_level: i32) -> [f64; 3] {
        find_spawn_location(sea_level, |x, z| self.surface_height(x, z))
    }

    /// Climat de la colonne ; `None` quand l'univers n'en a pas.
    fn climate_at(&self, _wx: i32, _wz: i32) -> Option<Climate> {
        None
    }

    /// Règle le pas de saison (0..8, [`crate::seasons`]) des chunks générés ensuite ; `None` :
    /// état de fin d'été, sans surcouche. L'état est partagé par toutes les copies (`fork`).
    /// Les univers sans saisons l'ignorent.
    fn set_season(&self, _step: Option<u8>) {}

    /// Pas de saison courant.
    fn season(&self) -> Option<u8> {
        None
    }

    /// Recalcule la surcouche saisonnière d'un chunk existant sans toucher aux colonnes que le
    /// joueur a modifiées. `true` si le contenu a changé.
    fn reseason(&self, _chunk: &mut Chunk) -> bool {
        false
    }

    /// Table d'eau des 16 × 16 colonnes du chunk `(cx, cz)`, `[lz * 16 + lx]` (nappes infinies et
    /// courant des rivières). `None` : l'univers n'a pas d'eau vivante.
    fn water_table(&self, _cx: i32, _cz: i32) -> Option<Box<crate::fluid::WaterTable>> {
        None
    }
}

/// Drapeaux de systèmes optionnels : le code teste un drapeau, jamais l'univers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Systems {
    pub seasons: bool,
    pub mys: bool,
    pub freshness: bool,
    pub folklore: bool,
    pub air_medium: bool,
    /// Eau à niveaux, courants et sources (doc 07) ; faux pour vanilla.
    pub living_water: bool,
}

/// Tout ce qui dépend de l'univers d'un monde, construit au chargement et partagé (`Arc`).
///
/// Cette session ne porte que les biomes, le générateur et les drapeaux de systèmes ; les blocs,
/// objets, recettes, mobs et règles viendront avec la palette par monde (session 0.3).
pub struct WorldProfile {
    pub kind: WorldKind,
    /// Table `BiomeId → BiomeInfo`.
    pub biomes: Vec<BiomeInfo>,
    pub systems: Systems,
    /// Rayon du carré de colonnes dont le mailleur moyenne les teintes de biome (0 : teinte du
    /// biome de la colonne, comme vanilla ; 2 : moyenne sur 5 × 5 colonnes).
    pub tint_blend: i32,
    generator: Arc<dyn TerrainGenerator>,
}

impl fmt::Debug for WorldProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorldProfile")
            .field("kind", &self.kind)
            .field("biomes", &self.biomes.len())
            .field("systems", &self.systems)
            .field("tint_blend", &self.tint_blend)
            .finish_non_exhaustive()
    }
}

impl WorldProfile {
    pub fn new(
        kind: WorldKind,
        biomes: Vec<BiomeInfo>,
        systems: Systems,
        generator: Arc<dyn TerrainGenerator>,
    ) -> Self {
        assert!(
            !biomes.is_empty() && biomes.len() <= usize::from(u8::MAX) + 1,
            "un profil porte de 1 à 256 biomes"
        );
        Self {
            kind,
            biomes,
            systems,
            tint_blend: 0,
            generator,
        }
    }

    /// Même profil, teintes moyennées sur un carré de rayon `radius` colonnes.
    pub fn with_tint_blend(mut self, radius: i32) -> Self {
        self.tint_blend = radius.max(0);
        self
    }

    /// Nouvelle instance de générateur (une par thread de génération).
    pub fn new_generator(&self) -> Box<dyn TerrainGenerator> {
        self.generator.fork()
    }

    /// Générateur partagé du profil, pour les requêtes ponctuelles (hauteur, biome).
    pub fn generator(&self) -> &Arc<dyn TerrainGenerator> {
        &self.generator
    }

    /// Description d'un biome ; un identifiant inconnu retombe sur le premier biome du profil.
    pub fn biome(&self, id: BiomeId) -> &BiomeInfo {
        self.biomes.get(usize::from(id)).unwrap_or(&self.biomes[0])
    }

    /// Identifiant d'un biome par son nom.
    pub fn biome_id_by_name(&self, name: &str) -> Option<BiomeId> {
        self.biomes
            .iter()
            .position(|b| b.name == name)
            .map(|i| i as BiomeId)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_kind_names_round_trip() {
        for k in WorldKind::ALL {
            assert_eq!(WorldKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(WorldKind::parse(" Norrcraft "), Some(WorldKind::Norrcraft));
        assert_eq!(WorldKind::parse("autre"), None);
        assert_eq!(WorldKind::default(), WorldKind::Vanilla);
        assert_eq!(
            serde_json::to_string(&WorldKind::Norrcraft).unwrap(),
            "\"norrcraft\""
        );
    }
}
