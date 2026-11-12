//! Contexte de tick, générateurs aléatoires, événements, trait `System` et ordonnancement
//! (spec §5, §19.3, §19.4).

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use nb_core::glam::{DVec3, IVec3};
use nb_core::JavaRandom;
use nb_data::{BlockId, BlockRegistry, ItemId, MobRegistry, SurvivalData};
use nb_world::WorldConfig;

use crate::blocks::BlockAccess;
use crate::components::EntityId;
use crate::input::PlayerInput;
use crate::world::EntityWorld;

/// Un `java.util.Random` par système, comme en Java.
#[derive(Debug, Clone)]
pub struct EcsRngs {
    /// `MobSpawnSystem`.
    pub spawn: JavaRandom,
    /// `PassiveMobAISystem` (sert aussi à `triggerPlayerFlee`).
    pub passive: JavaRandom,
    /// `AggressiveMobAISystem`.
    pub aggressive: JavaRandom,
    /// RNG du `World` (particules ambiantes).
    pub ambient: JavaRandom,
    /// Moteur d'états du bestiaire Norrcraft (`BrainSystem`).
    pub brain: JavaRandom,
}

impl EcsRngs {
    /// Graines déterministes (tests de conformité, rejouabilité).
    pub fn from_seed(seed: i64) -> Self {
        Self {
            spawn: JavaRandom::new(seed),
            passive: JavaRandom::new(seed.wrapping_add(1)),
            aggressive: JavaRandom::new(seed.wrapping_add(2)),
            ambient: JavaRandom::new(seed.wrapping_add(3)),
            brain: JavaRandom::new(seed.wrapping_add(4)),
        }
    }

    /// Graines non déterministes, comme `new Random()` en Java (horloge + compteur).
    pub fn from_entropy() -> Self {
        static UNIQUIFIER: AtomicI64 = AtomicI64::new(8_682_522_807_148_012);
        let next = || {
            let u = UNIQUIFIER
                .fetch_add(1_181_783_497_276_652_981, Ordering::Relaxed)
                .wrapping_mul(1_181_783_497_276_652_981);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as i64);
            JavaRandom::new(u ^ nanos)
        };
        Self {
            spawn: next(),
            passive: next(),
            aggressive: next(),
            ambient: next(),
            brain: next(),
        }
    }
}

/// Événements émis pendant un tick (pour nb-game : sons, remaillage, UI…).
#[derive(Clone, PartialEq, Debug)]
pub enum EcsEvent {
    /// Bloc cassé par le joueur (`block` = bloc retiré).
    BlockBroken { pos: IVec3, block: BlockId },
    /// Bloc posé par le joueur.
    BlockPlaced { pos: IVec3, block: BlockId },
    /// Clic droit sur un bloc-station (établi, fonderie) : l'interface ouvre son écran.
    StationUsed {
        pos: IVec3,
        station: nb_data::Station,
    },
    /// Mob frappé par le joueur.
    MobHit { entity: EntityId },
    /// Mob apparu.
    MobSpawned { entity: EntityId },
    /// Entité non joueur détruite par le `HealthSystem` (mort).
    MobDied { entity: EntityId },
    /// Mob détruit par éloignement.
    MobDespawned { entity: EntityId },
    /// Le joueur est mort (suivi immédiatement de [`EcsEvent::PlayerRespawned`]).
    PlayerDied,
    /// Le joueur vient de réapparaître (mort ou `/kill`).
    PlayerRespawned,
    /// Minage en cours (tous les ~0,25 s) : son et éclats de matière.
    BlockMining {
        pos: IVec3,
        block: BlockId,
        progress: f32,
    },
    /// Coup porté à un mob avec `damage` (pour la réplication LAN).
    AttackDealt { entity: EntityId, damage: f32 },
    /// Message à afficher au joueur.
    Notice(String),
    /// Objets gagnés (butin, récolte, pêche).
    Gained { item: ItemId, count: i32 },
    /// Outil ou armure cassé.
    ItemBroke { item: ItemId },
    /// Pièce d'armure équipée.
    Equipped { item: ItemId },
    /// Terre labourée à la houe.
    Tilled { pos: IVec3 },
    /// Graine ou plant mis en terre.
    Planted { pos: IVec3 },
    /// Ligne lancée.
    Cast { pos: DVec3 },
    /// Ligne relevée.
    Reel { pos: DVec3 },
    /// Un poisson mord.
    FishBite { pos: DVec3 },
    /// Prise ramenée.
    Caught { item: ItemId },
    /// Bouchée en cours.
    Eating { item: ItemId },
    /// Repas terminé.
    Ate { item: ItemId },
    /// Le joueur souffre du froid ou de la faim.
    Hurt { cold: bool },
    /// Une culture est passée au stade suivant.
    CropGrew { pos: IVec3, block: BlockId },
    /// La glace sous un pied craque : elle ne portera pas la charge (6.1).
    IceCrack { pos: IVec3 },
    /// La glace a cédé en `pos` (6.1).
    IceBroke { pos: IVec3 },
    /// « Whoumf » sourd du manteau neigeux : une avalanche va partir (6.2).
    Whoumf { pos: IVec3 },
    /// Une avalanche est partie de `pos` : colonnes balayées, volume déposé en huitièmes (6.2).
    Avalanche {
        pos: IVec3,
        columns: u32,
        volume: u32,
    },
    /// Acte d'un animal Norrcraft sur le monde (broutage, construction, vol, mise à mort).
    AnimalAct {
        entity: EntityId,
        act: crate::brain::AnimalAct,
        pos: IVec3,
    },
}

/// Tout ce dont les systèmes ont besoin pendant un tick (aucun état global).
pub struct TickCtx<'a> {
    pub blocks: &'a mut dyn BlockAccess,
    pub block_reg: &'a BlockRegistry,
    pub mobs: &'a MobRegistry,
    /// Configuration vivante du monde (gameplay, §18).
    pub config: &'a mut WorldConfig,
    pub input: &'a PlayerInput,
    pub rngs: &'a mut EcsRngs,
    pub events: &'a mut Vec<EcsEvent>,
    /// Règles du mode survie (`None` : comportement d'origine, sans faim ni minage progressif).
    pub survival: Option<&'a SurvivalData>,
    /// Météo vécue par le joueur local (air, vent, pluie) dans les univers qui en ont une ;
    /// `None` : la chaleur vient du biome et de l'heure, comme avant la session 3.3.
    pub environment: Option<&'a nb_world::weather::Environment>,
}

impl TickCtx<'_> {
    /// Le bloc est-il solide (`blocks.json`) ?
    #[inline]
    pub fn is_solid(&self, id: BlockId) -> bool {
        self.block_reg.from_id(id).solid
    }
}

/// Un système de l'ECS.
pub trait System {
    fn name(&self) -> &'static str;
    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32);
}

/// Liste ordonnée de systèmes (`addSystem`, `addSystemFirst`, `updateSystems`).
#[derive(Default)]
pub struct Schedule {
    systems: Vec<Box<dyn System>>,
}

impl std::fmt::Debug for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

impl Schedule {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ordre réel du Java après `initPlayer` : PlayerInput, Physics, PassiveAI, AggressiveAI,
    /// MobSpawn, Health. (`PlayerInputSystem` ne fait rien tant qu'il n'y a pas de joueur, ce
    /// qui équivaut à son insertion en tête par `initPlayer`.)
    pub fn java_default() -> Self {
        let mut s = Self::new();
        s.push(Box::new(crate::player::PlayerInputSystem));
        s.push(Box::new(crate::physics::PhysicsSystem));
        s.push(Box::new(crate::ai::PassiveAiSystem));
        s.push(Box::new(crate::ai::AggressiveAiSystem));
        s.push(Box::new(crate::spawn::MobSpawnSystem::default()));
        s.push(Box::new(crate::health::HealthSystem));
        s
    }

    /// Ordonnancement d'un client LAN : entrée du joueur local seulement. Mobs, apparitions et
    /// santé des mobs viennent de l'hôte (réplication).
    pub fn client() -> Self {
        let mut s = Self::new();
        s.push(Box::new(crate::player::PlayerInputSystem));
        s.push(Box::new(crate::survival::VitalsSystem));
        s.push(Box::new(crate::health::PlayerHealthSystem));
        s
    }

    /// `addSystem`.
    pub fn push(&mut self, s: Box<dyn System>) {
        self.systems.push(s);
    }

    /// Insère un système à la position `index` (borne : fin de liste).
    pub fn insert(&mut self, index: usize, s: Box<dyn System>) {
        let i = index.min(self.systems.len());
        self.systems.insert(i, s);
    }

    /// `addSystemFirst`.
    pub fn push_front(&mut self, s: Box<dyn System>) {
        self.systems.insert(0, s);
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.systems.iter().map(|s| s.name()).collect()
    }

    /// `updateSystems(dt)`.
    pub fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32) {
        for s in &mut self.systems {
            s.run(world, ctx, dt);
        }
    }
}
