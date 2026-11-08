//! Identité des sons : matériaux des blocs, voix des créatures, effets.

/// Famille sonore d'un bloc (casse, pose, pas).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Material {
    Stone,
    Dirt,
    Grass,
    Gravel,
    Sand,
    Snow,
    Wood,
    Leaves,
    Plant,
    Glass,
    Cloth,
    Metal,
    Ice,
    Water,
}

impl Material {
    pub const ALL: [Material; 14] = [
        Material::Stone,
        Material::Dirt,
        Material::Grass,
        Material::Gravel,
        Material::Sand,
        Material::Snow,
        Material::Wood,
        Material::Leaves,
        Material::Plant,
        Material::Glass,
        Material::Cloth,
        Material::Metal,
        Material::Ice,
        Material::Water,
    ];

    /// Matériau d'après le nom d'un bloc de `blocks.json` (`None` pour l'air).
    pub fn from_block_name(name: &str) -> Option<Material> {
        use Material::*;
        let has = |s: &str| name.contains(s);
        Some(match name {
            "air" => return None,
            "water" | "lava" => Water,
            "dirt" | "clay" | "podzol" | "mycelium" | "farmland" | "dirt_path" | "ash" | "moss" => {
                Dirt
            }
            "grass" => Grass,
            "gravel" => Gravel,
            "sand" | "red_sand" => Sand,
            "snow" => Snow,
            "ice" | "packed_ice" => Ice,
            "glass" | "sea_lantern" | "lantern" => Glass,
            "gold_block" | "iron_block" | "diamond_block" | "emerald_block" | "aurora_block"
            | "forge" => Metal,
            "hay_block" => Cloth,
            "cobweb" => Cloth,
            "campfire" | "torch" | "bookshelf" | "workbench" | "pumpkin" | "bamboo" => Wood,
            "cactus" | "vines" | "reeds" | "tall_grass" | "fern" | "dead_bush" | "wheat"
            | "lily_pad" | "berry_bush" | "cattail" | "desert_sage" | "pampas_grass"
            | "cotton_grass" | "lavender" | "hibiscus" | "ember_bloom" => Plant,
            _ if has("flower") || has("orchid") || has("allium") || has("sunflower") => Plant,
            _ if has("lily") || has("mushroom_brown") || has("mushroom_red") => Plant,
            _ if has("leaves") => Leaves,
            _ if has("wood") || has("planks") || has("mushroom_stem") || has("mushroom_cap") => {
                Wood
            }
            _ if has("wool") => Cloth,
            _ if has("coral") => Stone,
            _ => Stone,
        })
    }
}

/// Timbre de base d'une créature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Voice {
    /// Grognement porcin / ours.
    Grunt,
    /// Mugissement bas (bison, chameau).
    Moo,
    /// Bêlement (chèvre, mouflon).
    Bleat,
    /// Gloussement d'oiseau terrestre.
    Cluck,
    /// Cri strident (flamant, manchot, autruche).
    Squawk,
    /// Grondement de prédateur (loup, panthère).
    Growl,
    /// Petit couinement (lièvre, hérisson, panda roux).
    Squeak,
    /// Coassement.
    Croak,
    /// Sifflement de reptile.
    Hiss,
    /// Cliquetis de carapace (crabe, scorpion).
    Chitter,
    /// Chant de cétacé.
    Whale,
    /// Bulles de poisson.
    Bubble,
    /// Poche gluante (méduse, calamar).
    Squelch,
    /// Hennissement / barrissement (zèbre, girafe, cerf).
    Whinny,
}

/// Type de cri.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MobCall {
    Idle,
    Hurt,
    Death,
}

/// Voix d'après l'identifiant d'un mob, avec un facteur de hauteur (grand = grave).
pub fn voice_for_mob(id: &str) -> (Voice, f32) {
    use Voice::*;
    let (v, p) = match id {
        "wild_boar" | "mossback" | "capybara" | "tortoise" | "reindeer" => (Grunt, 1.0),
        "brown_bear" | "polar_bear" => (Grunt, 0.55),
        "elk" | "giraffe" | "zebra" | "camel" => (Whinny, 1.0),
        "bison" => (Moo, 0.9),
        "mountain_goat" | "mouflon" => (Bleat, 1.0),
        "pheasant" => (Cluck, 1.0),
        "flamingo" | "penguin" | "ostrich" => (Squawk, 1.0),
        "grey_wolf" | "panther" => (Growl, 0.8),
        "arctic_fox" | "fennec_fox" | "red_panda" => (Squeak, 0.8),
        "hare" | "hedgehog" => (Squeak, 1.2),
        "frog" => (Croak, 1.0),
        "sand_lizard" | "ember_newt" => (Hiss, 1.0),
        "crab" | "scorpion" => (Chitter, 1.0),
        "dolphin" | "narwhal" | "sea_turtle" => (Whale, 1.3),
        "orca" | "shark" | "manta_ray" => (Whale, 0.8),
        "fish" | "clownfish" | "salmon" | "tuna" | "pufferfish" | "lanternfish" | "piranha" => {
            (Bubble, 1.0)
        }
        "jellyfish" | "ink_squid" => (Squelch, 1.0),
        _ => (Grunt, 1.0),
    };
    (v, p)
}

/// Un effet sonore identifiable (et mis en cache par la banque).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Break(Material),
    Place(Material),
    Step(Material),
    Land(Material),
    /// Éclaboussure d'entrée dans l'eau.
    Splash,
    /// Brasse en nageant.
    Swim,
    /// Coup porté (poing) sur une créature.
    Hit,
    PlayerHurt,
    PlayerDeath,
    Respawn,
    Mob(Voice, MobCall),
    Pickup,
    Craft,
    UiClick,
    UiOpen,
    UiClose,
    UiSlider,
    /// Goutte tombant dans une grotte.
    Drip,
    /// Chant d'oiseau (variantes).
    Bird,
    /// Gouttes / craquement de lave.
    Crackle,
}

impl Sfx {
    /// Nombre de variantes générées (tirées au hasard à la lecture).
    pub fn variants(self) -> usize {
        match self {
            Sfx::Break(_) | Sfx::Place(_) => 3,
            Sfx::Step(_) | Sfx::Land(_) => 5,
            Sfx::Mob(_, MobCall::Idle) => 3,
            Sfx::Mob(..) => 2,
            Sfx::Bird => 6,
            Sfx::Drip => 3,
            Sfx::Splash | Sfx::Swim | Sfx::Hit | Sfx::Crackle => 3,
            _ => 1,
        }
    }

    /// Nom de fichier recherché dans le dossier de sons personnalisés (sans extension), pour
    /// remplacer un son procédural par un vrai fichier `.wav`.
    pub fn file_stem(self) -> String {
        fn mat(m: Material) -> &'static str {
            match m {
                Material::Stone => "stone",
                Material::Dirt => "dirt",
                Material::Grass => "grass",
                Material::Gravel => "gravel",
                Material::Sand => "sand",
                Material::Snow => "snow",
                Material::Wood => "wood",
                Material::Leaves => "leaves",
                Material::Plant => "plant",
                Material::Glass => "glass",
                Material::Cloth => "cloth",
                Material::Metal => "metal",
                Material::Ice => "ice",
                Material::Water => "water",
            }
        }
        match self {
            Sfx::Break(m) => format!("break_{}", mat(m)),
            Sfx::Place(m) => format!("place_{}", mat(m)),
            Sfx::Step(m) => format!("step_{}", mat(m)),
            Sfx::Land(m) => format!("land_{}", mat(m)),
            Sfx::Splash => "splash".into(),
            Sfx::Swim => "swim".into(),
            Sfx::Hit => "hit".into(),
            Sfx::PlayerHurt => "player_hurt".into(),
            Sfx::PlayerDeath => "player_death".into(),
            Sfx::Respawn => "respawn".into(),
            Sfx::Mob(v, c) => format!(
                "mob_{}_{}",
                format!("{v:?}").to_lowercase(),
                match c {
                    MobCall::Idle => "idle",
                    MobCall::Hurt => "hurt",
                    MobCall::Death => "death",
                }
            ),
            Sfx::Pickup => "pickup".into(),
            Sfx::Craft => "craft".into(),
            Sfx::UiClick => "ui_click".into(),
            Sfx::UiOpen => "ui_open".into(),
            Sfx::UiClose => "ui_close".into(),
            Sfx::UiSlider => "ui_slider".into(),
            Sfx::Drip => "drip".into(),
            Sfx::Bird => "bird".into(),
            Sfx::Crackle => "crackle".into(),
        }
    }
}

/// Canal de mixage (un curseur de volume chacun, plus le volume général).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bus {
    Music,
    Ambient,
    /// Blocs, pas du joueur, interface.
    Effects,
    /// Créatures et autres joueurs.
    Creatures,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_names_map_to_materials() {
        assert_eq!(Material::from_block_name("air"), None);
        assert_eq!(Material::from_block_name("stone"), Some(Material::Stone));
        assert_eq!(Material::from_block_name("grass"), Some(Material::Grass));
        assert_eq!(
            Material::from_block_name("birch_leaves"),
            Some(Material::Leaves)
        );
        assert_eq!(
            Material::from_block_name("dark_oak_wood"),
            Some(Material::Wood)
        );
        assert_eq!(
            Material::from_block_name("planks_oak"),
            Some(Material::Wood)
        );
        assert_eq!(Material::from_block_name("wool_red"), Some(Material::Cloth));
        assert_eq!(
            Material::from_block_name("flower_blue"),
            Some(Material::Plant)
        );
        assert_eq!(Material::from_block_name("iron_ore"), Some(Material::Stone));
    }

    #[test]
    fn all_builtin_mobs_have_a_voice() {
        for id in [
            "wild_boar",
            "fish",
            "orca",
            "frog",
            "jellyfish",
            "unknown_mob",
        ] {
            let (_, p) = voice_for_mob(id);
            assert!(p > 0.0);
        }
    }

    #[test]
    fn file_stems_are_unique() {
        let mut set = std::collections::HashSet::new();
        for m in Material::ALL {
            for s in [Sfx::Break(m), Sfx::Place(m), Sfx::Step(m), Sfx::Land(m)] {
                assert!(set.insert(s.file_stem()));
            }
        }
    }
}
