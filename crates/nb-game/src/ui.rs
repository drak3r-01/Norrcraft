//! Façade d'interface : nb-game ne connaît pas nb-ui. À chaque tick, l'écran de l'état courant
//! reçoit une vue en lecture ([`UiView`]) et les entrées, et renvoie des [`UiCommand`] que le
//! jeu exécute (les clics de boutons du Java). nb-ui implémentera [`Ui`] à l'intégration ;
//! [`StubUi`] (clavier, sans rendu) permet de jouer en attendant.

use nb_data::keys;
use nb_data::{GameSettings, MobRegistry};
use nb_ecs::{GameMode, Inventory, ItemStack};
use nb_world::save::WorldMetadata;
use nb_world::WorldConfig;

use crate::chat::ChatMessage;
use crate::input::InputManager;
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
    /// « Apply » des packs : pack choisi (rechargé s'il diffère), puis SETTINGS.
    ApplyResourcePack(String),
    /// Registre de mobs modifié et sauvegardé (`MobRegistry.save()` : `dirty = true`).
    SetMobRegistry(Box<MobRegistry>),
    /// Sortie de l'écran des mobs (`applyAndLeave`) → SETTINGS.
    LeaveMobSettings,
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
    pub resource_packs: &'a [String],
    pub mobs: &'a MobRegistry,
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
}

/// Écrans (nb-ui). Toutes les méthodes ont un défaut neutre.
pub trait Ui {
    /// `State.enter` côté interface.
    fn enter(&mut self, _state: GameState, _view: &UiView<'_>, _input: &mut InputManager) {}
    /// `State.exit` côté interface (fermer la saisie, oublier un glisser…).
    fn exit(&mut self, _state: GameState, _input: &mut InputManager) {}
    /// Une fois par tick dans l'état courant, après la logique clavier du jeu.
    fn update(
        &mut self,
        _view: &UiView<'_>,
        _input: &mut InputManager,
        _dt: f64,
    ) -> Vec<UiCommand> {
        Vec::new()
    }
}

/// Aucune interface (tests, serveur).
#[derive(Clone, Copy, Debug, Default)]
pub struct NullUi;

impl Ui for NullUi {}

/// Écrans provisoires au clavier, sans rendu, en attendant nb-ui :
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
