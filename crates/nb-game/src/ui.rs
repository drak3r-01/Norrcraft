//! Façade d'interface. À chaque tick, l'écran de l'état courant reçoit une vue en lecture
//! ([`UiView`]) et les entrées, et renvoie des [`UiCommand`] que le jeu exécute (les clics de
//! boutons du Java) ; à chaque frame il remplit une liste de dessin 2D ([`Ui::draw`]).
//! Implémentations : [`crate::screens::NbUi`] (les écrans de nb-ui, celle du jeu), [`NullUi`]
//! (rien) et [`StubUi`] (clavier, sans rendu : tests et outils).

use nb_data::keys;
use nb_data::{BlockRegistry, GameSettings, ItemId, ItemRegistry, MobRegistry, Station};
use nb_ecs::{Button, CraftingGrid, GameMode, Inventory, ItemStack};
use nb_gfx::ui_pass::{UiDraw, UiTextureSlot, UiVertex};
use nb_gfx::ResourcePackMeta;
use nb_mesh::BlockTable;
use nb_ui::{DrawList, TextureRef};
use nb_world::save::WorldMetadata;
use nb_world::WorldConfig;

use crate::chat::ChatMessage;
use crate::input::InputManager;
use crate::net::LanView;
use crate::state::{GameState, HudState, SettingsTab};

/// Demande de création de monde (`WorldSelectState`, bouton « Create »).
#[derive(Clone, Debug, PartialEq)]
pub struct NewWorld {
    /// Nom saisi (rogné à la Java par le jeu).
    pub name: String,
    /// Graine saisie (vide = aléatoire, entier, sinon `hashCode` du texte).
    pub seed_text: String,
    pub game_mode: GameMode,
    pub allow_cheats: bool,
    pub config: WorldConfig,
}

/// Actions déclenchées par l'interface.
#[derive(Clone, Debug)]
pub enum UiCommand {
    /// `stateManager.switchState(s)` sans autre effet (PLAY → WORLD_SELECT, annuler…).
    SwitchState(GameState),
    /// Bouton « SETTINGS » du menu ou de la pause : `previousState = état courant`.
    OpenSettings,
    /// Onglet des écrans packs / mobs vers SETTINGS (`pendingSettingsTab`).
    OpenSettingsTab(SettingsTab),
    /// Retour / ÉCHAP des réglages : `switchState(previousState)`.
    SettingsBack,
    /// « Apply » des réglages : copie, application, sauvegarde, puis `previousState`.
    ApplySettings(Box<GameSettings>),
    /// « RESUME » (pause) → PLAYING.
    Resume,
    /// « MAIN MENU » (pause) : sauvegarde puis MAIN_MENU.
    SaveAndMainMenu,
    /// « QUIT ».
    Quit,
    /// Jouer un monde de la liste.
    PlayWorld(Box<WorldMetadata>),
    CreateWorld(Box<NewWorld>),
    /// Éditer : renommer + mode + triche.
    EditWorld {
        name: String,
        new_name: String,
        game_mode: GameMode,
        allow_cheats: bool,
    },
    DeleteWorld(String),
    /// « Re-Create » : copie des chunks et de la config sous un nouveau nom, puis jouer.
    RecreateWorld {
        source: String,
        new_name: String,
    },
    /// Relire la liste des mondes.
    RefreshWorlds,
    /// Clic gauche sur une case d'inventaire (`-1` = hors grille).
    InventoryClick(i32),
    /// Livre de recettes : remplir la grille de craft avec la recette `n` du livre.
    RecipeFill(usize),
    /// Clic sur une case d'un écran d'inventaire / de station (voir `nb_ecs::crafting` pour la
    /// numérotation) : bouton, Maj.
    SlotClick {
        slot: i32,
        button: Button,
        shift: bool,
    },
    /// « Apply » des packs : pack choisi (rechargé s'il diffère), puis SETTINGS.
    ApplyResourcePack(String),
    /// Registre de mobs modifié et sauvegardé (`MobRegistry.save()` : `dirty = true`).
    SetMobRegistry(Box<MobRegistry>),
    /// Mob ajouté : `assetManager.reloadMobModel(id)` (relayé à la plateforme).
    ReloadMobModel(String),
    /// « Reset to Defaults » confirmé : registre intégré sauvegardé, tous les mobs supprimés,
    /// `clearDirty`.
    ResetMobRegistry(Box<MobRegistry>),
    /// Sortie de l'écran des mobs (`applyAndLeave`) → SETTINGS.
    LeaveMobSettings,
    /// Menu pause « OPEN TO LAN » : le monde courant devient une partie multijoueur.
    OpenToLan,
    /// Menu pause « CLOSE LAN » : retour au solo, les invités sont renvoyés.
    CloseLan,
    /// Écran Multiplayer : rejoindre `address` (`hôte[:port]`) sous le nom `name`.
    JoinLan {
        address: String,
        name: String,
    },
    /// Écran Multiplayer : annuler la connexion en cours.
    CancelJoin,
    /// Écran Multiplayer : relancer la recherche des parties.
    RefreshLan,
}

/// Couche d'atlas de l'icône de chaque objet pur (les blocs utilisent la table de maillage).
#[derive(Clone, Debug, Default)]
pub struct ItemIcons(std::collections::HashMap<ItemId, u32>);

impl ItemIcons {
    /// `layer` : couche d'atlas d'une texture, d'après son nom.
    pub fn build(items: &ItemRegistry, layer: impl Fn(&str) -> u32) -> Self {
        Self(
            items
                .iter()
                .filter(|i| i.block.is_none())
                .map(|i| (i.id, layer(&i.texture)))
                .collect(),
        )
    }

    /// Couche de l'objet (0 si inconnu).
    pub fn get(&self, item: ItemId) -> u32 {
        self.0.get(&item).copied().unwrap_or(0)
    }
}

/// Vue en lecture du jeu pour l'écran courant.
#[derive(Clone, Debug)]
pub struct UiView<'a> {
    pub state: GameState,
    pub window_size: (u32, u32),
    pub settings: &'a GameSettings,
    /// Mondes sauvegardés (relus à l'entrée de WORLD_SELECT et après chaque modification).
    pub worlds: &'a [WorldMetadata],
    /// Packs disponibles (« Default » d'abord), relus à l'entrée de RESOURCE_PACKS.
    pub resource_packs: &'a [ResourcePackMeta],
    pub mobs: &'a MobRegistry,
    /// Noms des blocs (`formatBlockName`).
    pub blocks: &'a BlockRegistry,
    /// Couches de texture des blocs (icônes : `getTextureIndex(0)`).
    pub block_table: &'a BlockTable,
    /// Objets (noms des objets purs : bâton, lingot…).
    pub items: &'a ItemRegistry,
    /// Règles de survie (outils, nourriture, armures) pour les infobulles.
    pub survival: &'a nb_data::SurvivalData,
    /// Toutes les recettes (livre de recettes).
    pub recipes: &'a nb_data::RecipeBook,
    /// État de survie du joueur.
    pub vitals: Option<nb_ecs::Vitals>,
    /// Messages à afficher (texte, secondes restantes).
    pub notices: &'a [(String, f32)],
    /// Butin récent (nom, quantité, secondes restantes).
    pub pickups: &'a [(String, i32, f32)],
    /// Couches de texture des objets purs.
    pub item_icons: &'a ItemIcons,
    /// Station affichée par l'écran d'inventaire (main, établi, fonderie).
    pub craft_station: Station,
    /// Grille de craft de l'écran d'inventaire.
    pub craft_grid: &'a CraftingGrid,
    pub previous_state: GameState,
    /// Onglet à l'entrée de SETTINGS.
    pub settings_tab: SettingsTab,
    pub hud: &'a HudState,
    /// Copie de l'inventaire du joueur.
    pub inventory: Option<Inventory>,
    /// Objet tenu au curseur dans l'écran d'inventaire.
    pub held: Option<ItemStack>,
    pub selected_slot: u8,
    pub game_mode: Option<GameMode>,
    /// `(santé, santé max)` du joueur.
    pub health: Option<(f32, f32)>,
    /// Journal du chat à afficher (vide = rien) ; le texte saisi est dans
    /// `InputManager::command_buffer` quand `chat_open`.
    pub chat_log: &'a [ChatMessage],
    pub chat_open: bool,
    pub current_world: Option<&'a str>,
    pub saves_root: &'a std::path::Path,
    /// Partie en réseau local : rôle, joueurs, parties trouvées, état de la connexion.
    pub lan: &'a LanView,
}

/// Écrans (nb-ui). Toutes les méthodes ont un défaut neutre.
pub trait Ui {
    /// `State.enter` côté interface.
    fn enter(&mut self, _state: GameState, _view: &UiView<'_>, _input: &mut InputManager) {}
    /// `State.exit` côté interface (fermer la saisie, oublier un glisser…).
    fn exit(&mut self, _state: GameState, _input: &mut InputManager) {}
    /// Écouteur de réaffectation (`setRebindListener`) : touche capturée par
    /// [`InputManager::start_rebind`], livrée par le jeu **avant** [`Ui::update`] du même tick
    /// (le callback GLFW du Java passe avant la mise à jour de l'état).
    fn key_pressed(&mut self, _state: GameState, _key: i32) {}
    /// Une fois par tick dans l'état courant, après la logique clavier du jeu.
    fn update(
        &mut self,
        _view: &UiView<'_>,
        _input: &mut InputManager,
        _dt: f64,
    ) -> Vec<UiCommand> {
        Vec::new()
    }
    /// Dessin 2D de la frame (après la scène 3D) : écran de l'état puis chat.
    fn draw(&mut self, _view: &UiView<'_>, _input: &InputManager, _dl: &mut DrawList) {}
}

/// Liste de dessin 2D convertie pour `nb_gfx::ui_pass::UiRenderer::record`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiFrame {
    /// Taille logique (fenêtre) de la liste.
    pub width: u32,
    pub height: u32,
    pub vertices: Vec<UiVertex>,
    pub draws: Vec<UiDraw>,
}

impl UiFrame {
    /// `nb_ui::Vertex` → [`UiVertex`] (même disposition, `bytemuck`) ; `Batch` → [`UiDraw`]
    /// (texture UI = index de `nb_ui::textures::load_all`).
    pub fn from_draw_list(dl: &DrawList) -> Self {
        let vertices: &[UiVertex] = bytemuck::cast_slice(dl.vertices());
        let draws = dl
            .batches()
            .iter()
            .map(|b| UiDraw {
                texture: match b.texture {
                    TextureRef::Ui(t) => UiTextureSlot::Image(t.index()),
                    TextureRef::BlockLayer(l) => UiTextureSlot::BlockLayer(l),
                },
                scissor: b.scissor.map(|s| [s.x, s.y, s.w, s.h]),
                first: b.first,
                count: b.count,
            })
            .collect();
        Self {
            width: dl.width,
            height: dl.height,
            vertices: vertices.to_vec(),
            draws,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

/// Aucune interface (tests, serveur).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullUi;

impl Ui for NullUi {}

/// Écrans au clavier, sans rendu (tests, outils ; le jeu utilise [`crate::NbUi`]) :
/// MAIN_MENU : Entrée → choix du monde, S → réglages, F10 → quitter ;
/// WORLD_SELECT : Entrée → premier monde (ou création de « My World »), ÉCHAP → menu ;
/// PAUSED : Entrée → reprendre, M → sauvegarder et menu, S → réglages ;
/// SETTINGS / RESOURCE_PACKS / MOB_SETTINGS : ÉCHAP → retour.
#[derive(Clone, Debug, Default)]
pub struct StubUi {
    prev_keys: Vec<i32>,
}

const KEY_M: i32 = 77;
const KEY_F10: i32 = 299;

impl StubUi {
    pub fn new() -> Self {
        Self::default()
    }

    fn just(&self, input: &InputManager, code: i32) -> bool {
        input.is_key_down(code) && !self.prev_keys.contains(&code)
    }
}

impl Ui for StubUi {
    fn enter(&mut self, _state: GameState, _view: &UiView<'_>, input: &mut InputManager) {
        // Une touche encore enfoncée à l'entrée ne déclenche rien.
        self.prev_keys = WATCHED
            .iter()
            .copied()
            .filter(|&k| input.is_key_down(k))
            .collect();
    }

    fn update(&mut self, view: &UiView<'_>, input: &mut InputManager, _dt: f64) -> Vec<UiCommand> {
        let mut out = Vec::new();
        if !input.is_typing_mode() {
            let enter = self.just(input, keys::KEY_ENTER);
            let esc = self.just(input, keys::KEY_ESCAPE);
            match view.state {
                GameState::MainMenu => {
                    if enter {
                        out.push(UiCommand::SwitchState(GameState::WorldSelect));
                    } else if self.just(input, keys::KEY_S) {
                        out.push(UiCommand::OpenSettings);
                    } else if self.just(input, KEY_F10) {
                        out.push(UiCommand::Quit);
                    }
                }
                GameState::WorldSelect => {
                    if enter {
                        out.push(match view.worlds.first() {
                            Some(m) => UiCommand::PlayWorld(Box::new(m.clone())),
                            None => UiCommand::CreateWorld(Box::new(NewWorld {
                                name: "My World".into(),
                                seed_text: String::new(),
                                game_mode: GameMode::Survival,
                                allow_cheats: false,
                                config: WorldConfig::default(),
                            })),
                        });
                    } else if esc {
                        out.push(UiCommand::SwitchState(GameState::MainMenu));
                    }
                }
                GameState::Paused => {
                    if enter {
                        out.push(UiCommand::Resume);
                    } else if self.just(input, KEY_M) {
                        out.push(UiCommand::SaveAndMainMenu);
                    } else if self.just(input, keys::KEY_S) {
                        out.push(UiCommand::OpenSettings);
                    }
                }
                GameState::Settings if esc => out.push(UiCommand::SettingsBack),
                GameState::ResourcePacks if esc => {
                    out.push(UiCommand::OpenSettingsTab(SettingsTab::Graphics))
                }
                GameState::MobSettings if esc => out.push(UiCommand::LeaveMobSettings),
                _ => {}
            }
        }
        self.prev_keys = WATCHED
            .iter()
            .copied()
            .filter(|&k| input.is_key_down(k))
            .collect();
        out
    }
}

const WATCHED: [i32; 5] = [
    keys::KEY_ENTER,
    keys::KEY_ESCAPE,
    keys::KEY_S,
    KEY_M,
    KEY_F10,
];
