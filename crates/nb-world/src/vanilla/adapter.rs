//! Adaptateur vanilla (plan Norrcraft, session 0.2) : branche `WorldGenerator` et l'enum
//! [`Biome`] sur les contrats communs de `profile.rs`. Les deux `if GeneratorKind` restent à
//! l'intérieur de `WorldGenerator` ; rien ne change dans la génération.

use std::sync::Arc;

use nb_data::BlockIds;

use crate::chunk::Chunk;
use crate::profile::{
    BiomeAmbience, BiomeId, BiomeInfo, Climate, Precipitation, Systems, TerrainGenerator,
    WorldKind, WorldProfile,
};

use super::biome::{Biome, BiomeSystem};
use super::config::WorldConfig;
use super::generator::WorldGenerator;

/// `WorldGenerator` vu comme [`TerrainGenerator`] ; l'identifiant de biome est l'ordinal de
/// [`Biome`].
#[derive(Debug, Clone)]
pub struct VanillaTerrain {
    generator: WorldGenerator,
}

impl VanillaTerrain {
    pub fn new(generator: WorldGenerator) -> Self {
        Self { generator }
    }

    pub fn generator(&self) -> &WorldGenerator {
        &self.generator
    }
}

impl TerrainGenerator for VanillaTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(self.clone())
    }

    fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.generator.compute_surface_height(wx, wz)
    }

    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId {
        self.generator.biome_system().biome_at(wx, wz).ordinal() as BiomeId
    }

    fn generate_chunk(&self, chunk: &mut Chunk) {
        self.generator.generate_chunk(chunk);
    }

    fn climate_at(&self, wx: i32, wz: i32) -> Option<Climate> {
        let bs = self.generator.biome_system();
        let (x, z) = (f64::from(wx), f64::from(wz));
        Some(Climate {
            temperature: bs.temperature(x, z),
            humidity: bs.humidity(x, z),
            latitude: None,
        })
    }
}

impl WorldProfile {
    /// Profil de l'univers vanilla : 30 biomes dans l'ordre de l'enum, aucun système optionnel.
    pub fn vanilla(seed: i64, cfg: Arc<WorldConfig>, ids: &BlockIds) -> Self {
        let generator = WorldGenerator::new(seed, cfg, ids);
        Self::vanilla_with(generator)
    }

    /// Comme [`WorldProfile::vanilla`] autour d'un générateur déjà construit.
    pub fn vanilla_with(generator: WorldGenerator) -> Self {
        let biomes = biome_infos(generator.biome_system());
        Self::new(
            WorldKind::Vanilla,
            biomes,
            Systems::default(),
            Arc::new(VanillaTerrain::new(generator)),
        )
    }
}

/// Table des 30 biomes vanilla, dans l'ordre de [`Biome::ALL`] (= identifiants de biome).
pub fn biome_infos(sys: &BiomeSystem) -> Vec<BiomeInfo> {
    Biome::ALL
        .iter()
        .map(|&b| {
            let c = sys.config(b);
            let (music, amb) = sound_of(b);
            let [wind, birds, crickets, frogs, water, lava] = amb;
            BiomeInfo {
                name: b.name().to_owned(),
                grass_tint: c.grass_tint,
                leaves_tint: c.leaves_tint,
                water_color: c.water_color,
                temperature: temperature_of(b),
                precipitation: precipitation_of(b),
                music: music.to_owned(),
                ambience: BiomeAmbience {
                    wind,
                    birds,
                    crickets,
                    frogs,
                    water,
                    lava,
                },
            }
        })
        .collect()
}

/// Température indicative (0 glacial, 1 torride), calée sur les seuils de `getBiome`.
fn temperature_of(b: Biome) -> f32 {
    use Biome::*;
    match b {
        IceSpikes | SnowyPeaks => 0.0,
        FrozenOcean | SnowyPlains => 0.05,
        SnowyTaiga => 0.1,
        Taiga => 0.25,
        Mountains | MegaTaiga => 0.3,
        AutumnForest | Hills | StonyShore => 0.4,
        DarkForest | Heath | DeepOcean => 0.45,
        Plains | Forest | Swamp | Ocean | River | BirchForest | CherryGrove | MushroomFields => 0.5,
        FlowerMeadow => 0.55,
        Beach => 0.6,
        Savanna => 0.8,
        Jungle | WarmOcean => 0.85,
        Desert => 0.9,
        Badlands => 0.95,
        Volcano => 1.0,
    }
}

fn precipitation_of(b: Biome) -> Precipitation {
    use Biome::*;
    match b {
        SnowyPlains | SnowyTaiga | IceSpikes | FrozenOcean | SnowyPeaks => Precipitation::Snow,
        Desert | Badlands | Volcano => Precipitation::None,
        _ => Precipitation::Rain,
    }
}

/// Thème musical et ambiances `[vent, oiseaux, grillons, grenouilles, eau, lave]` d'un biome
/// (anciennes tables `biome_theme` / `biome_ambience` de `nb-game/src/audio.rs`).
fn sound_of(b: Biome) -> (&'static str, [f32; 6]) {
    use Biome::*;
    let music = match b {
        Plains | FlowerMeadow | Heath | Savanna | Hills | Beach | River => "meadow",
        Forest | BirchForest | AutumnForest | Taiga | MegaTaiga => "forest",
        DarkForest | CherryGrove | MushroomFields => "mystic",
        Jungle => "jungle",
        Desert | Badlands => "desert",
        Swamp => "swamp",
        SnowyPlains | SnowyTaiga | IceSpikes | FrozenOcean => "snow",
        Mountains | SnowyPeaks | StonyShore => "mountain",
        Ocean | DeepOcean | WarmOcean => "ocean",
        Volcano => "volcano",
    };
    let amb = match b {
        Plains | Savanna | Heath | Hills => [0.3, 0.6, 0.7, 0.0, 0.0, 0.0],
        FlowerMeadow | CherryGrove => [0.2, 1.0, 0.6, 0.0, 0.0, 0.0],
        Forest | BirchForest | AutumnForest | MegaTaiga => [0.2, 1.0, 0.6, 0.0, 0.0, 0.0],
        DarkForest => [0.25, 0.3, 0.5, 0.0, 0.0, 0.0],
        Taiga => [0.4, 0.4, 0.2, 0.0, 0.0, 0.0],
        Jungle => [0.1, 1.0, 1.0, 0.5, 0.0, 0.0],
        Swamp => [0.15, 0.3, 0.8, 1.0, 0.0, 0.0],
        Desert | Badlands => [0.6, 0.0, 0.3, 0.0, 0.0, 0.0],
        SnowyPlains | SnowyTaiga | IceSpikes => [0.7, 0.0, 0.0, 0.0, 0.0, 0.0],
        Mountains | SnowyPeaks => [0.8, 0.0, 0.0, 0.0, 0.0, 0.0],
        Ocean | DeepOcean | WarmOcean | FrozenOcean => [0.35, 0.0, 0.0, 0.0, 1.0, 0.0],
        River => [0.15, 0.4, 0.4, 0.0, 0.45, 0.0],
        Beach => [0.3, 0.2, 0.0, 0.0, 0.7, 0.0],
        StonyShore => [0.5, 0.0, 0.0, 0.0, 0.7, 0.0],
        Volcano => [0.5, 0.0, 0.0, 0.0, 0.0, 0.7],
        MushroomFields => [0.15, 0.2, 0.7, 0.0, 0.0, 0.0],
    };
    (music, amb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::BlockRegistry;

    #[test]
    fn vanilla_profile_lists_all_biomes_in_enum_order() {
        let reg = BlockRegistry::builtin();
        let p = WorldProfile::vanilla(7, Arc::new(WorldConfig::default()), reg.ids());
        assert_eq!(p.kind, WorldKind::Vanilla);
        assert_eq!(p.biomes.len(), Biome::ALL.len());
        for (i, b) in Biome::ALL.iter().enumerate() {
            assert_eq!(p.biomes[i].name, b.name());
            assert_eq!(p.biome_id_by_name(b.name()), Some(i as BiomeId));
        }
        assert_eq!(p.systems, Systems::default());
    }

    #[test]
    fn adapter_matches_the_underlying_generator() {
        let reg = BlockRegistry::builtin();
        let cfg = Arc::new(WorldConfig::default());
        let g = WorldGenerator::new(12345, cfg.clone(), reg.ids());
        let p = WorldProfile::vanilla(12345, cfg, reg.ids());
        let t = p.new_generator();
        for (x, z) in [(0, 0), (100, -300), (-5000, 1234), (77, 77)] {
            assert_eq!(t.surface_height(x, z), g.compute_surface_height(x, z));
            assert_eq!(
                usize::from(t.biome_at(x, z)),
                g.biome_system().biome_at(x, z).ordinal()
            );
            assert_eq!(p.generator().biome_at(x, z), t.biome_at(x, z));
        }
        assert!(t.climate_at(0, 0).is_some());
    }
}
