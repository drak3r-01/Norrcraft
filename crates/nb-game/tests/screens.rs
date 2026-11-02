//! Écrans nb-ui branchés sur le jeu ([`NbUi`]) : parcours complet à la souris et au clavier,
//! sans GPU (menu → choix du monde → création → jeu → pause → menu → rechargement), dessin
//! converti pour nb-gfx, réaffectation de touche.

use std::path::PathBuf;
use std::sync::Arc;

use nb_data::keys;
use nb_game::input::glfw;
use nb_game::{Game, GameMode, GameOptions, GameState, NbUi, Registries, UiFrame, TIME_STEP};
use nb_gfx::ui_pass::UiTextureSlot;
use nb_mesh::BlockTable;
use nb_ui::UiTexture;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-ui-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn game(dir: &TempDir) -> Game {
    let settings = dir.0.join("settings.json");
    if !settings.exists() {
        std::fs::write(&settings, r#"{ "renderDistance": 2 }"#).unwrap();
    }
    let regs = Registries::builtin();
    // Couche = identifiant du bloc + 100 : vérifiable dans les lots d'icônes.
    let table = Arc::new(BlockTable::from_registry(&regs.blocks, |n| {
        100 + Registries::builtin().blocks.get(n).id as u32
    }));
    let mut o = GameOptions::new(&dir.0);
    o.menu_seed = Some(1234);
    o.rng_seed = Some(99);
    o.mesh_threads = Some(1);
    let mut g = Game::new(o, regs, table);
    g.set_ui(Box::new(NbUi::new()));
    g
}

fn tick(g: &mut Game) {
    g.update(TIME_STEP);
    g.input.end_frame();
}

/// Clic gauche en (x, y) : appui sur un tick, relâchement au suivant.
fn click(g: &mut Game, x: f64, y: f64) {
    g.input.cursor_moved(x, y);
    g.input.mouse_button(glfw::MOUSE_BUTTON_LEFT, true);
    tick(g);
    g.input.mouse_button(glfw::MOUSE_BUTTON_LEFT, false);
    tick(g);
}

fn press(g: &mut Game, code: i32) {
    g.input.key_event(code, true, false);
    tick(g);
    g.input.key_event(code, false, false);
    tick(g);
}

fn type_text(g: &mut Game, s: &str) {
    for c in s.chars() {
        g.input.char_event(c);
    }
    tick(g);
}

fn uses(f: &UiFrame, t: UiTexture) -> bool {
    f.draws
        .iter()
        .any(|d| d.texture == UiTextureSlot::Image(t.index()))
}

// Boutons du menu principal (1280×720) : x 440..840, y 360 / 420 / 480 (400×50).
const PLAY: (f64, f64) = (640.0, 380.0);
// Choix du monde : rangée 1 y 602..652, rangée 2 y 660..700, contenu x 240..1040.
const ROW1: f64 = 610.0;
const CREATE_NEW: (f64, f64) = (700.0, ROW1);
const CREATE_CONFIRM: (f64, f64) = (300.0, ROW1);
const SEED_FIELD: (f64, f64) = (300.0, 180.0);
// Pause : RESUME y 180, SETTINGS y 240, MAIN MENU y 300.
const MAIN_MENU_BTN: (f64, f64) = (640.0, 320.0);
const PAUSE_SETTINGS: (f64, f64) = (640.0, 260.0);

#[test]
fn full_flow_with_mouse_and_keyboard() {
    let dir = TempDir::new("flow");
    let mut g = game(&dir);
    tick(&mut g); // le panneau du menu est construit au premier `update`
    let f = g.draw_ui();
    assert!(uses(&f, UiTexture::Logo) && uses(&f, UiTexture::Button), "menu dessiné");
    assert_eq!((f.width, f.height), (1280, 720));
    let total: u32 = f.draws.iter().map(|d| d.count).sum();
    assert_eq!(total as usize, f.vertices.len());

    click(&mut g, PLAY.0, PLAY.1);
    assert_eq!(g.state(), GameState::WorldSelect);
    assert!(!GameState::WorldSelect.renders_world());
    let f = g.draw_ui();
    assert!(uses(&f, UiTexture::Dirt), "fond de terre");
    assert!(f.draws.iter().any(|d| d.scissor.is_some()), "liste découpée");

    click(&mut g, CREATE_NEW.0, CREATE_NEW.1);
    assert!(g.input.is_typing_mode());
    assert_eq!(g.input.command_buffer(), "My World");
    // Saisie du nom puis du germe (le germe saisi est utilisé : correctif fix_seed_input).
    for _ in 0.."My World".len() {
        g.input.key_event(keys::KEY_BACKSPACE, true, false);
        g.input.key_event(keys::KEY_BACKSPACE, false, false);
    }
    type_text(&mut g, "Terre");
    assert_eq!(g.input.command_buffer(), "Terre");
    click(&mut g, SEED_FIELD.0, SEED_FIELD.1);
    type_text(&mut g, "424242");
    assert_eq!(g.input.secondary_buffer(), "424242");
    // T n'ouvre pas le chat pendant la saisie.
    assert!(!g.chat().is_active());

    click(&mut g, CREATE_CONFIRM.0, CREATE_CONFIRM.1);
    assert_eq!(g.state(), GameState::Playing);
    assert!(!g.input.is_typing_mode());
    assert_eq!(g.session().name.as_deref(), Some("Terre"));
    assert_eq!(g.session().world.seed(), 424242);
    let meta = nb_world::save::load_metadata(&dir.0, "Terre").unwrap();
    assert_eq!(meta.game_mode, "SURVIVAL");

    // HUD : viseur, barre, sélecteur, cœurs (survie).
    let f = g.draw_ui();
    for t in [
        UiTexture::Crosshair,
        UiTexture::Hotbar,
        UiTexture::Selector,
        UiTexture::HungerFull,
    ] {
        assert!(uses(&f, t), "{t:?} attendu");
    }
    // Icône de bloc : couche `getTextureIndex(0)` de la table de maillage.
    let stone = g.regs.blocks.ids().stone;
    g.session_mut()
        .inventory_mut()
        .unwrap()
        .set_slot(27, Some(nb_ecs::ItemStack::new(stone, 3)));
    let f = g.draw_ui();
    let layer = g.block_table().get(stone).texture_layers[0];
    assert_eq!(layer, 100 + u32::from(stone));
    assert!(f
        .draws
        .iter()
        .any(|d| d.texture == UiTextureSlot::BlockLayer(layer)));

    // Inventaire : E ouvre, clic sur la case 27 (barre) prend la pile, E la repose.
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Inventory);
    let f = g.draw_ui();
    assert!(uses(&f, UiTexture::Inventory));
    click(&mut g, 360.0 + 5.0, 566.0 + 5.0);
    assert_eq!(g.inventory_cursor().held.map(|h| h.count), Some(3));
    let f = g.draw_ui();
    assert!(f
        .draws
        .iter()
        .any(|d| d.texture == UiTextureSlot::BlockLayer(layer)));
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Playing);
    assert!(g.inventory_cursor().held.is_none());

    // Chat : T ouvre, journal et zone de saisie dessinés.
    let before = g.draw_ui().vertices.len();
    press(&mut g, glfw::KEY_T);
    assert!(g.chat().is_active());
    type_text(&mut g, "time set 100");
    let open = g.draw_ui().vertices.len();
    assert!(open > before, "zone de saisie dessinée");
    press(&mut g, keys::KEY_ENTER);
    assert!(!g.chat().is_active());
    assert_eq!(g.chat().log().len(), 2);
    assert!(g.draw_ui().vertices.len() > before, "journal visible 5 s");

    // Pause → réglages → retour → menu principal (monde sauvegardé puis déchargé).
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Paused);
    let f = g.draw_ui();
    assert_eq!(f.vertices[0].color, [0.0, 0.0, 0.0, 0.6], "voile de pause");
    click(&mut g, PAUSE_SETTINGS.0, PAUSE_SETTINGS.1);
    assert_eq!(g.state(), GameState::Settings);
    assert_eq!(g.previous_state(), GameState::Paused);
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Paused);
    click(&mut g, MAIN_MENU_BTN.0, MAIN_MENU_BTN.1);
    assert_eq!(g.state(), GameState::MainMenu);
    assert!(g.session().name.is_none());
    g.shutdown();

    // Rechargement : jeu neuf, sélection de « Terre » dans la liste puis « Play ».
    let mut g = game(&dir);
    click(&mut g, PLAY.0, PLAY.1);
    assert_eq!(g.worlds().len(), 1);
    click(&mut g, 640.0, 64.0 + 8.0 + 20.0);
    click(&mut g, 300.0, ROW1);
    assert_eq!(g.state(), GameState::Playing);
    assert_eq!(g.session().name.as_deref(), Some("Terre"));
    assert_eq!(g.session().world.seed(), 424242);
    let inv = g.session().inventory().unwrap();
    assert_eq!(inv.slot(27).map(|s| s.count), Some(3));
    assert_eq!(nb_ecs::GameMode::Survival, {
        let p = g.session().player().unwrap();
        g.session()
            .sim
            .entities
            .copy::<nb_ecs::Player>(p)
            .unwrap()
            .game_mode
    });
    g.shutdown();
}

// Réglages (1280×720) : onglet Controls x 496..636, y 74..110 ; ligne « Forward » : bouton
// x 506..626, y 236..272.
const CONTROLS_TAB: (f64, f64) = (560.0, 90.0);
const FORWARD_BTN: (f64, f64) = (550.0, 250.0);

#[test]
fn key_rebind_goes_through_listener_before_update() {
    let dir = TempDir::new("rebind");
    let mut g = game(&dir);
    // Menu → SETTINGS (bouton 2).
    click(&mut g, 640.0, 440.0);
    assert_eq!(g.state(), GameState::Settings);
    click(&mut g, CONTROLS_TAB.0, CONTROLS_TAB.1);
    click(&mut g, FORWARD_BTN.0, FORWARD_BTN.1);
    assert!(g.input.is_rebinding(), "écouteur posé");
    // La touche suivante (G) est livrée à l'écran, puis Apply.
    press(&mut g, 71);
    assert!(!g.input.is_rebinding());
    // Apply : x 645..835, y = h - 80 + 15.
    click(&mut g, 700.0, 660.0);
    assert_eq!(g.state(), GameState::MainMenu);
    assert_eq!(
        g.settings.key_bindings.get(&nb_data::Action::Forward),
        Some(&71)
    );

    // Échap pendant une réaffectation : consommé par l'écouteur, puis revu enfoncé par
    // l'écran qui quitte les réglages (bogue Java conservé), sans rien lier.
    click(&mut g, 640.0, 440.0);
    click(&mut g, CONTROLS_TAB.0, CONTROLS_TAB.1);
    click(&mut g, FORWARD_BTN.0, FORWARD_BTN.1);
    assert!(g.input.is_rebinding());
    g.input.key_event(keys::KEY_ESCAPE, true, false);
    tick(&mut g);
    assert_eq!(g.state(), GameState::MainMenu, "Échap quitte l'écran");
    g.input.key_event(keys::KEY_ESCAPE, false, false);
    tick(&mut g);
    assert!(!g.input.is_rebinding());
    assert_eq!(
        g.settings.key_bindings.get(&nb_data::Action::Forward),
        Some(&71)
    );
    g.shutdown();
}

#[test]
fn ui_only_states_and_world_params() {
    // Préréglages : FLAT, puis champs édités par-dessus.
    let p = nb_game::screens::params_from_config(&nb_world::WorldConfig::preset(
        nb_world::WorldType::Flat,
    ));
    assert_eq!(p.sea_level, 5);
    let mut edited = p.clone();
    edited.coal_chance = 0.02;
    edited.sea_level = 7;
    let c = nb_game::screens::config_for(nb_ui::screens::WorldType::Flat, &edited);
    assert_eq!(c.sea_level, 7);
    assert_eq!(c.ores.coal.chance, 0.02);
    assert!(!c.caves_enabled, "champ du préréglage conservé");
    assert_eq!(
        nb_game::screens::params_from_config(&c),
        edited,
        "aller-retour"
    );
    let renders: Vec<_> = GameState::ALL
        .iter()
        .filter(|s| !s.renders_world())
        .collect();
    assert_eq!(
        renders,
        [
            &GameState::WorldSelect,
            &GameState::Settings,
            &GameState::ResourcePacks,
            &GameState::MobSettings
        ]
    );
    let _ = GameMode::Creative;
}
