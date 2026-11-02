//! Tests headless de nb-game : transitions d'états, ordre de mise à jour, chat, inventaire,
//! scénario complet créer / jouer / sauvegarder / recharger.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_data::keys;
use nb_ecs::{GameMode, ItemStack, Transform};
use nb_game::input::glfw;
use nb_game::{
    Game, GameOptions, GameState, InputManager, NewWorld, PlatformRequest, Registries, SettingsTab,
    Step, Ui, UiCommand, UiView, TIME_STEP,
};
use nb_mesh::BlockTable;
use nb_world::WorldConfig;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-game-{tag}-{}-{nanos}", std::process::id()));
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
    // Mondes légers pour les tests : distance de rendu 2 via `settings.json`.
    let settings = dir.0.join("settings.json");
    if !settings.exists() {
        std::fs::write(&settings, r#"{ "renderDistance": 2 }"#).unwrap();
    }
    let regs = Registries::builtin();
    let table = Arc::new(BlockTable::from_registry(&regs.blocks, |_| 0));
    let mut o = GameOptions::new(&dir.0);
    o.menu_seed = Some(1234);
    o.rng_seed = Some(99);
    o.mesh_threads = Some(1);
    let g = Game::new(o, regs, table);
    assert_eq!(g.settings.render_distance, 2);
    g
}

fn press(g: &mut Game, code: i32) {
    g.input.key_event(code, true, false);
    g.update(TIME_STEP);
    g.input.key_event(code, false, false);
    g.update(TIME_STEP);
}

fn new_world(name: &str, mode: GameMode) -> UiCommand {
    UiCommand::CreateWorld(Box::new(NewWorld {
        name: name.into(),
        seed_text: "12345".into(),
        game_mode: mode,
        allow_cheats: true,
        config: WorldConfig::default(),
    }))
}

#[test]
fn starts_in_main_menu_with_rotating_camera() {
    let dir = TempDir::new("menu");
    let mut g = game(&dir);
    assert_eq!(g.state(), GameState::MainMenu);
    assert!(g.session().name.is_none());
    g.update(TIME_STEP);
    g.update(TIME_STEP);
    let c = g.camera();
    assert_eq!(c.position, nb_core::glam::Vec3::new(0.0, 150.0, 0.0));
    assert_eq!(c.pitch_deg, 20.0);
    let expected = 2.0 * (TIME_STEP * 10.0) as f32;
    assert!((g.menu_rotation() - expected).abs() < 1e-6);
    assert_eq!(c.yaw_deg, g.menu_rotation());
    // Le menu relâche la souris ; la plateforme reçoit aussi les réglages graphiques.
    let req = g.take_requests();
    assert!(req.contains(&PlatformRequest::SetCursorGrab(false)));
    assert!(req
        .iter()
        .any(|r| matches!(r, PlatformRequest::ApplyGraphics(_))));
    g.shutdown();
}

#[test]
fn state_transitions_follow_java() {
    let dir = TempDir::new("states");
    let mut g = game(&dir);

    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    assert_eq!(g.state(), GameState::WorldSelect);
    // Sortie du menu : caméra remise en (0, 100, 0), rotation nulle.
    assert_eq!(g.camera().position.y, 100.0);
    assert_eq!(g.camera().pitch_deg, 0.0);
    g.command(UiCommand::SwitchState(GameState::MainMenu));

    // Réglages depuis le menu : retour au menu.
    g.command(UiCommand::OpenSettings);
    assert_eq!(g.state(), GameState::Settings);
    assert_eq!(g.previous_state(), GameState::MainMenu);
    g.command(UiCommand::SwitchState(GameState::ResourcePacks));
    g.command(UiCommand::OpenSettingsTab(SettingsTab::Controls));
    assert_eq!(g.state(), GameState::Settings);
    assert_eq!(g.ui_view().settings_tab, SettingsTab::Controls);
    g.command(UiCommand::SwitchState(GameState::MobSettings));
    g.command(UiCommand::LeaveMobSettings);
    assert_eq!(g.state(), GameState::Settings);
    assert_eq!(g.ui_view().settings_tab, SettingsTab::Graphics);
    g.command(UiCommand::SettingsBack);
    assert_eq!(g.state(), GameState::MainMenu);

    // Jouer.
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    g.command(new_world("Alpha", GameMode::Creative));
    assert_eq!(g.state(), GameState::Playing);
    assert_eq!(g.session().name.as_deref(), Some("Alpha"));
    assert!(g
        .take_requests()
        .contains(&PlatformRequest::SetCursorGrab(true)));

    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Paused);
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Playing);
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Inventory);
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Playing);
    press(&mut g, keys::KEY_E);
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Playing);

    // Réglages depuis la pause : retour à la pause.
    press(&mut g, keys::KEY_ESCAPE);
    g.command(UiCommand::OpenSettings);
    assert_eq!(g.previous_state(), GameState::Paused);
    g.command(UiCommand::ApplySettings(Box::new(g.settings.clone())));
    assert_eq!(g.state(), GameState::Paused);
    g.command(UiCommand::Resume);
    assert_eq!(g.state(), GameState::Playing);

    // Pause → menu : sauvegarde puis déchargement (correctif ; le Java le laissait chargé).
    press(&mut g, keys::KEY_ESCAPE);
    g.command(UiCommand::SaveAndMainMenu);
    assert_eq!(g.state(), GameState::MainMenu);
    assert!(dir.0.join("Alpha").join("world.json").is_file());
    assert!(g.session().name.is_none(), "monde déchargé");
    assert_eq!(g.session().world.seed(), 1234, "monde de menu neuf");
    // Le monde de menu n'est jamais sauvegardé : le dossier « Alpha » peut disparaître.
    std::fs::remove_dir_all(dir.0.join("Alpha")).unwrap();
    g.save_current();
    assert!(!dir.0.join("Alpha").exists());

    g.command(UiCommand::Quit);
    assert!(g.quit_requested());
    assert!(g.take_requests().contains(&PlatformRequest::Quit));
    g.shutdown();
}

#[derive(Default)]
struct Recorder {
    log: Rc<RefCell<Vec<String>>>,
}

impl Ui for Recorder {
    fn enter(&mut self, s: GameState, _v: &UiView<'_>, _i: &mut InputManager) {
        self.log.borrow_mut().push(format!("enter {s:?}"));
    }
    fn exit(&mut self, s: GameState, _i: &mut InputManager) {
        self.log.borrow_mut().push(format!("exit {s:?}"));
    }
    fn update(&mut self, v: &UiView<'_>, _i: &mut InputManager, _dt: f64) -> Vec<UiCommand> {
        self.log.borrow_mut().push(format!("update {:?}", v.state));
        if v.state == GameState::MainMenu {
            vec![UiCommand::SwitchState(GameState::WorldSelect)]
        } else {
            Vec::new()
        }
    }
}

#[test]
fn ui_facade_enter_update_exit() {
    let dir = TempDir::new("ui");
    let mut g = game(&dir);
    let log = Rc::new(RefCell::new(Vec::new()));
    g.set_ui(Box::new(Recorder { log: log.clone() }));
    g.update(TIME_STEP);
    g.update(TIME_STEP);
    assert_eq!(
        *log.borrow(),
        vec![
            "enter MainMenu",
            "update MainMenu",
            "exit MainMenu",
            "enter WorldSelect",
            "update WorldSelect",
        ]
    );
    g.shutdown();
}

#[test]
fn update_order_matches_main() {
    let dir = TempDir::new("order");
    let mut g = game(&dir);
    g.enable_trace();
    g.update(TIME_STEP);
    assert_eq!(
        g.take_trace(),
        vec![
            Step::Input,
            Step::Time,
            Step::Chat,
            Step::State(GameState::MainMenu),
            Step::Chunks,
            Step::Meshes
        ]
    );
    g.command(new_world("Order", GameMode::Creative));
    g.resize(800, 600);
    g.update(TIME_STEP);
    assert_eq!(
        g.take_trace(),
        vec![
            Step::Input,
            Step::Time,
            Step::Chat,
            Step::Resize,
            Step::State(GameState::Playing),
            Step::EcsTick,
            Step::Chunks,
            Step::Meshes
        ]
    );
    assert!((g.render_view().camera.aspect - 800.0 / 600.0).abs() < 1e-6);
    // PAUSED : pas d'ECS, mais l'horloge avance toujours.
    press(&mut g, keys::KEY_ESCAPE);
    g.take_trace();
    let t0 = g.sky().time_of_day;
    let cam0 = g.camera();
    g.update(TIME_STEP);
    let trace = g.take_trace();
    assert!(!trace.contains(&Step::EcsTick));
    assert!(trace.contains(&Step::State(GameState::Paused)));
    assert!(g.sky().time_of_day > t0);
    assert_eq!(g.camera(), cam0);
    g.shutdown();
}

#[test]
fn chat_runs_commands_before_ecs() {
    let dir = TempDir::new("chat");
    let mut g = game(&dir);
    // Monde du menu : pas de triche.
    press(&mut g, glfw::KEY_T);
    assert!(g.chat().is_active());
    for c in "kill".chars() {
        g.input.char_event(c);
    }
    press(&mut g, keys::KEY_ENTER);
    assert!(!g.chat().is_active());
    let log: Vec<_> = g.chat().log().iter().map(|m| m.text.clone()).collect();
    assert_eq!(log, vec!["/kill", "Cheats are not enabled in this world."]);

    // Monde avec triche : /time set agit sur l'horloge.
    g.command(new_world("Cheats", GameMode::Creative));
    press(&mut g, glfw::KEY_T);
    for c in "time set 100".chars() {
        g.input.char_event(c);
    }
    press(&mut g, keys::KEY_ENTER);
    let t = g.sky().time_of_day;
    assert!((100.0..101.0).contains(&t), "{t}");
    g.shutdown();
}

#[test]
fn inventory_cursor_and_return_on_close() {
    let dir = TempDir::new("inv");
    let mut g = game(&dir);
    g.command(new_world("Inv", GameMode::Survival));
    let stone = g.regs.blocks.ids().stone;
    g.session_mut()
        .inventory_mut()
        .unwrap()
        .set_slot(3, Some(ItemStack::new(stone, 5)));
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Inventory);
    g.command(UiCommand::InventoryClick(3));
    assert_eq!(g.inventory_cursor().held.map(|h| h.count), Some(5));
    assert!(g.session().inventory().unwrap().slot(3).is_none());
    assert_eq!(g.ui_view().held.map(|h| h.block), Some(stone));
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Playing);
    assert!(g.inventory_cursor().held.is_none());
    assert_eq!(
        g.session().inventory().unwrap().slot(3).map(|s| s.count),
        Some(5)
    );
    g.shutdown();
}

#[test]
fn world_list_edit_delete_recreate() {
    let dir = TempDir::new("list");
    let mut g = game(&dir);
    g.command(new_world("One", GameMode::Creative));
    press(&mut g, keys::KEY_ESCAPE);
    g.command(UiCommand::SaveAndMainMenu);
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    assert_eq!(g.worlds().len(), 1);
    // Nom déjà pris : création refusée.
    g.command(new_world("One", GameMode::Creative));
    assert_eq!(g.state(), GameState::WorldSelect);
    g.command(UiCommand::EditWorld {
        name: "One".into(),
        new_name: "  Uno ".into(),
        game_mode: GameMode::Survival,
        allow_cheats: false,
    });
    assert_eq!(g.worlds()[0].name, "Uno");
    assert_eq!(g.worlds()[0].game_mode, "SURVIVAL");
    g.command(UiCommand::RecreateWorld {
        source: "Uno".into(),
        new_name: "Dos".into(),
    });
    assert_eq!(g.state(), GameState::Playing);
    assert_eq!(g.session().name.as_deref(), Some("Dos"));
    assert_eq!(g.session().world.seed(), 12345);
    press(&mut g, keys::KEY_ESCAPE);
    g.command(UiCommand::SaveAndMainMenu);
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    // Correctif (décision de Gabriel) : la pause ramène au menu en déchargeant le monde, donc
    // « One » renommé en « Uno » n'est plus resauvegardé sous son ancien nom par
    // `loadAndPlay` (le Java recréait `One/world.json` à côté de « Uno »).
    let mut names: Vec<_> = g.worlds().iter().map(|m| m.name.clone()).collect();
    names.sort();
    assert_eq!(names, ["Dos", "Uno"]);
    assert!(!dir.0.join("One").exists());
    // Supprimer un monde déjà quitté ne le fait pas renaître.
    g.command(UiCommand::DeleteWorld("Uno".into()));
    g.command(UiCommand::SwitchState(GameState::MainMenu));
    g.command(UiCommand::Quit);
    assert!(!dir.0.join("Uno").exists());
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    assert_eq!(g.worlds().len(), 1);
    assert_eq!(g.worlds()[0].name, "Dos");
    g.shutdown();
}

#[test]
fn resource_pack_change_invalidates_meshes_and_waits_for_table() {
    let dir = TempDir::new("pack");
    let mut g = game(&dir);
    g.command(UiCommand::OpenSettings);
    g.command(UiCommand::SwitchState(GameState::ResourcePacks));
    let packs: Vec<_> = g
        .ui_view()
        .resource_packs
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(packs, ["Default"]);
    g.take_requests();
    g.command(UiCommand::ApplyResourcePack("Other".into()));
    assert_eq!(g.state(), GameState::Settings);
    assert_eq!(g.settings.resource_pack, "Other");
    assert!(g
        .take_requests()
        .contains(&PlatformRequest::ReloadResourcePack("Other".into())));
    let gen = g.mesh_scheduler().generation();
    assert!(gen > 0);
    let saved = std::fs::read_to_string(dir.0.join("settings.json")).unwrap();
    assert!(saved.contains("\"Other\""));
    let table = Arc::new(BlockTable::from_registry(&g.regs.blocks, |_| 0));
    g.set_block_table(table);
    g.update(TIME_STEP);
    // Même pack : rien à recharger.
    g.command(UiCommand::SwitchState(GameState::ResourcePacks));
    g.take_requests();
    g.command(UiCommand::ApplyResourcePack("Other".into()));
    assert!(!g
        .take_requests()
        .iter()
        .any(|r| matches!(r, PlatformRequest::ReloadResourcePack(_))));
    assert_eq!(g.mesh_scheduler().generation(), gen);
    g.shutdown();
}

fn wait_for_chunk(g: &mut Game, x: i32, z: i32) {
    let t = Instant::now();
    while !g.world().is_chunk_loaded(x, z) {
        assert!(t.elapsed() < Duration::from_secs(60), "chunk jamais chargé");
        g.update(TIME_STEP);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Scénario headless : créer un monde, avancer N ticks, modifier, sauvegarder, recharger.
#[test]
fn headless_create_play_save_reload() {
    let dir = TempDir::new("scenario");
    let (pos, time, block_at, stone, health) = {
        let mut g = game(&dir);
        g.command(UiCommand::SwitchState(GameState::WorldSelect));
        g.command(new_world("Scenario", GameMode::Survival));
        assert_eq!(g.state(), GameState::Playing);

        // Premier lancement : spawn cherché et mémorisé.
        let spawn = g.world().config().spawn.expect("spawn mémorisé");
        let p0 = g
            .session()
            .sim
            .entities
            .copy::<Transform>(g.session().player().unwrap());
        assert_eq!(p0.unwrap().pos.x, spawn[0]);

        let (sx, sz) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
        wait_for_chunk(&mut g, sx, sz);
        g.run_ticks(120);
        assert!(g.ticks() >= 120);

        // Rendu : vue construite sans GPU.
        let view = g.render_view();
        assert_eq!(view.state, GameState::Playing);
        let _ = view.frame_input(0.0);
        // Des maillages finissent par être produits.
        let t = Instant::now();
        let mut uploads = 0;
        while uploads == 0 && t.elapsed() < Duration::from_secs(60) {
            g.update(TIME_STEP);
            uploads += g
                .take_mesh_actions()
                .iter()
                .filter(|a| matches!(a, nb_mesh::MeshAction::Upload(..)))
                .count();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(uploads > 0, "aucun maillage produit");

        // Modifications (sans tick ECS ensuite) : un bloc, l'inventaire, la santé.
        let stone = g.regs.blocks.ids().stone;
        let b = (sx, 120, sz);
        g.world().set_block(b.0, b.1, b.2, stone);
        g.session_mut()
            .inventory_mut()
            .unwrap()
            .set_slot(27, Some(ItemStack::new(stone, 12)));
        let pid = g.session().player().unwrap();
        g.session_mut()
            .sim
            .entities
            .get_mut::<nb_ecs::Health>(pid)
            .unwrap()
            .health = 7.0;
        let pos = g.session().sim.entities.copy::<Transform>(pid).unwrap().pos;
        let time = g.sky().time_of_day;
        // ÉCHAP : le tick bascule en PAUSED avant l'ECS, le joueur ne bouge plus.
        press(&mut g, keys::KEY_ESCAPE);
        assert_eq!(g.state(), GameState::Paused);
        g.command(UiCommand::SaveAndMainMenu);
        assert!(g.take_error().is_none());
        g.shutdown();
        (pos, time, b, stone, 7.0f32)
    };

    // Rechargement dans un nouveau jeu.
    let mut g = game(&dir);
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    assert_eq!(g.worlds().len(), 1);
    let meta = g.worlds()[0].clone();
    assert_eq!(meta.seed, 12345);
    assert!(meta.allow_cheats);
    g.command(UiCommand::PlayWorld(Box::new(meta)));
    assert_eq!(g.state(), GameState::Playing);
    let pid = g.session().player().unwrap();
    let e = &g.session().sim.entities;
    assert!((e.copy::<Transform>(pid).unwrap().pos - pos).length() < 1e-9);
    assert_eq!(e.copy::<nb_ecs::Health>(pid).unwrap().health, health);
    let cam = g.camera();
    assert!((f64::from(cam.position.y) - (pos.y + 1.62)).abs() < 1e-3);
    // L'horloge a avancé de quelques ticks après la mesure (pause), jamais reculé.
    assert!(g.sky().time_of_day >= time);
    let inv = g.session().inventory().unwrap();
    assert_eq!(inv.slot(27), Some(ItemStack::new(stone, 12)));
    wait_for_chunk(&mut g, block_at.0, block_at.2);
    assert_eq!(g.world().block(block_at.0, block_at.1, block_at.2), stone);
    g.shutdown();
}

#[test]
fn seeds_parse_like_java() {
    let mut r = nb_core::JavaRandom::new(1);
    assert_eq!(nb_game::parse_seed(" 42 ", &mut r), 42);
    assert_eq!(nb_game::parse_seed("-7", &mut r), -7);
    assert_eq!(nb_game::parse_seed("+7", &mut r), 7);
    assert_eq!(
        nb_game::parse_seed("hello", &mut r),
        i64::from(nb_world::java_string_hash("hello"))
    );
    let v = nb_game::parse_seed("", &mut r);
    assert!(v >= 0);
}

#[test]
fn fixed_step_drives_game_ticks() {
    let dir = TempDir::new("loop");
    let mut g = game(&dir);
    let mut l = nb_game::FixedStep::new();
    // 2,5 pas puis 0,6 pas : 2 puis 1 tick ; une frame énorme est bornée.
    for (frame, expected) in [(TIME_STEP * 2.5, 2), (TIME_STEP * 0.6, 1)] {
        let before = g.ticks();
        l.run(frame, |dt| g.update(dt));
        assert_eq!(g.ticks() - before, expected);
    }
    let before = g.ticks();
    l.run(30.0, |dt| g.update(dt));
    assert!(g.ticks() - before <= 15);
    g.shutdown();
}

#[test]
fn edited_mobs_are_despawned_when_leaving_mob_settings() {
    let dir = TempDir::new("mobs");
    let mut g = game(&dir);
    let e = &mut g.session_mut().sim.entities;
    let mob = e.create();
    e.insert(mob, nb_ecs::MobType::new("reindeer"));
    e.insert(mob, Transform::new(0.0, 80.0, 0.0));
    assert_eq!(g.render_view().mobs.len(), 1);
    let view = g.render_view();
    let fi = view.frame_input(0.0);
    assert_eq!(fi.mobs.len(), 1, "mob transmis à nb-gfx");
    assert_eq!(fi.mobs[0].model_id, "reindeer");
    drop(view);
    g.command(UiCommand::OpenSettings);
    g.command(UiCommand::SwitchState(GameState::MobSettings));
    // Sans modification : rien n'est supprimé.
    g.command(UiCommand::LeaveMobSettings);
    assert!(g.session().sim.entities.contains(mob));
    g.command(UiCommand::SwitchState(GameState::MobSettings));
    let reg = g.regs.mobs.clone();
    g.command(UiCommand::SetMobRegistry(Box::new(reg)));
    assert!(dir.0.join("mobs.json").is_file());
    g.command(UiCommand::LeaveMobSettings);
    assert_eq!(g.state(), GameState::Settings);
    assert!(!g.session().sim.entities.contains(mob));
    assert!(g
        .take_requests()
        .contains(&PlatformRequest::ReloadMobModels));
    g.shutdown();
}
