//! Inventaire, établi et fonderie de bout en bout : vrais écrans nb-ui, vraie souris, vrai
//! monde (clic droit sur un établi posé, sauvegarde / rechargement des objets purs).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_data::{keys, Station};
use nb_ecs::{GameMode, ItemStack};
use nb_game::input::glfw;
use nb_game::{
    Game, GameOptions, GameState, ItemIcons, NbUi, NewWorld, Registries, UiCommand, UiFrame,
    TIME_STEP,
};
use nb_gfx::ui_pass::UiTextureSlot;
use nb_mesh::BlockTable;
use nb_ui::{InventoryScreen, UiTexture};
use nb_world::WorldConfig;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-craft-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Icône d'un bloc = 100 + identifiant ; icône d'un objet pur = 500 + identifiant d'objet.
const BLOCK_ICON: u32 = 100;
const ITEM_ICON: u32 = 500;

fn game(dir: &TempDir) -> Game {
    let settings = dir.0.join("settings.json");
    if !settings.exists() {
        std::fs::write(&settings, r#"{ "renderDistance": 2 }"#).unwrap();
    }
    let regs = Registries::builtin();
    let blocks = regs.blocks.clone();
    let table = Arc::new(BlockTable::from_registry(&regs.blocks, move |n| {
        BLOCK_ICON + u32::from(blocks.get(n).id)
    }));
    let icons = ItemIcons::build(&regs.items, |tex| {
        let items = Registries::builtin().items;
        let name = tex.strip_prefix("item_").unwrap();
        ITEM_ICON + u32::from(items.id_of(name).unwrap())
    });
    let mut o = GameOptions::new(&dir.0);
    o.menu_seed = Some(1234);
    o.rng_seed = Some(99);
    o.mesh_threads = Some(1);
    let mut g = Game::new(o, regs, table);
    g.set_item_icons(Arc::new(icons));
    g.set_ui(Box::new(NbUi::new()));
    g
}

fn new_world(name: &str) -> UiCommand {
    UiCommand::CreateWorld(Box::new(NewWorld {
        name: name.into(),
        seed_text: "777".into(),
        game_mode: GameMode::Survival,
        allow_cheats: true,
        config: WorldConfig::default(),
    }))
}

fn tick(g: &mut Game) {
    g.update(TIME_STEP);
    g.input.end_frame();
}

fn press(g: &mut Game, code: i32) {
    g.input.key_event(code, true, false);
    tick(g);
    g.input.key_event(code, false, false);
    tick(g);
}

/// Clic de souris en (x, y) ; `shift` maintient Maj pendant le clic.
fn click_at(g: &mut Game, (x, y): (f64, f64), button: usize, shift: bool) {
    g.input.cursor_moved(x, y);
    if shift {
        g.input.key_event(keys::KEY_LEFT_SHIFT, true, false);
    }
    g.input.mouse_button(button, true);
    tick(g);
    g.input.mouse_button(button, false);
    tick(g);
    if shift {
        g.input.key_event(keys::KEY_LEFT_SHIFT, false, false);
        tick(g);
    }
}

fn left(g: &mut Game, p: (f64, f64)) {
    click_at(g, p, glfw::MOUSE_BUTTON_LEFT, false);
}

fn right(g: &mut Game, p: (f64, f64)) {
    click_at(g, p, glfw::MOUSE_BUTTON_RIGHT, false);
}

/// Centre de la case `slot` de l'écran (fenêtre 1280×720 par défaut).
fn at(station: Station, slot: i32) -> (f64, f64) {
    let s = InventoryScreen::for_station(1280, 720, station);
    let (x, y) = s.slot_pixels_of(slot);
    (f64::from(x) + 28.0, f64::from(y) + 28.0)
}

fn uses(f: &UiFrame, t: UiTexture) -> bool {
    f.draws
        .iter()
        .any(|d| d.texture == UiTextureSlot::Image(t.index()))
}

fn layer_used(f: &UiFrame, layer: u32) -> bool {
    f.draws
        .iter()
        .any(|d| d.texture == UiTextureSlot::BlockLayer(layer))
}

fn item(g: &Game, name: &str) -> u16 {
    g.regs
        .items
        .id_of(name)
        .unwrap_or_else(|| panic!("objet {name}"))
}

fn count_of(g: &Game, name: &str) -> i32 {
    g.session().inventory().unwrap().count_of(item(g, name))
}

#[test]
fn hand_crafting_with_the_mouse_planks_to_sticks() {
    let dir = TempDir::new("hand");
    let mut g = game(&dir);
    g.command(new_world("Hand"));
    let planks = item(&g, "planks_oak");
    g.session_mut()
        .inventory_mut()
        .unwrap()
        .set_slot(0, Some(ItemStack::new(planks, 6)));
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Inventory);
    assert_eq!(g.craft_station(), Station::Hand);
    assert!(uses(&g.draw_ui(), UiTexture::Inventory));

    let s = Station::Hand;
    left(&mut g, at(s, 0)); // prend les 6 planches
    right(&mut g, at(s, nb_ecs::GRID_BASE)); // une planche en haut à gauche…
    right(&mut g, at(s, nb_ecs::GRID_BASE + 2)); // …et une en dessous
    assert_eq!(
        g.craft_grid().output(),
        Some(ItemStack::new(item(&g, "stick"), 4))
    );
    // Le résultat est dessiné avec l'icône de l'objet pur.
    assert!(layer_used(
        &g.draw_ui(),
        ITEM_ICON + u32::from(item(&g, "stick"))
    ));
    left(&mut g, at(s, 0)); // repose les 4 planches restantes
                            // Maj + clic sur le résultat : craft en série dans l'inventaire (la grille ne contient que
                            // deux planches : un seul craft).
    click_at(
        &mut g,
        at(s, nb_ecs::OUTPUT_SLOT),
        glfw::MOUSE_BUTTON_RIGHT,
        true,
    );
    assert_eq!(count_of(&g, "stick"), 4);
    assert_eq!(count_of(&g, "planks_oak"), 4);
    assert!(g.craft_grid().is_empty());
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Playing);
    assert_eq!(count_of(&g, "stick"), 4);
    assert_eq!(count_of(&g, "planks_oak"), 4);
    g.shutdown();
}

#[test]
fn closing_returns_the_grid_and_the_held_stack() {
    let dir = TempDir::new("close");
    let mut g = game(&dir);
    g.command(new_world("Close"));
    let (stone, dirt) = (item(&g, "stone"), item(&g, "dirt"));
    let inv = g.session_mut().inventory_mut().unwrap();
    inv.set_slot(0, Some(ItemStack::new(stone, 5)));
    inv.set_slot(1, Some(ItemStack::new(dirt, 9)));
    press(&mut g, keys::KEY_E);
    let s = Station::Hand;
    left(&mut g, at(s, 0));
    right(&mut g, at(s, nb_ecs::GRID_BASE + 1)); // 1 pierre dans la grille, 4 en main
    left(&mut g, at(s, 1)); // échange : la terre passe en main, 4 pierres reposées en case 1
    assert_eq!(g.inventory_cursor().held.map(|h| h.item), Some(dirt));
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(g.state(), GameState::Playing);
    assert!(g.craft_grid().is_empty());
    assert!(g.inventory_cursor().held.is_none());
    assert_eq!(count_of(&g, "stone"), 5, "aucune pierre perdue");
    assert_eq!(count_of(&g, "dirt"), 9, "aucune terre perdue");
    g.shutdown();
}

#[test]
fn workbench_screen_makes_a_forge_then_forge_smelts() {
    let dir = TempDir::new("station");
    let mut g = game(&dir);
    g.command(new_world("Station"));
    let (stone, coal, ore, plank) = (
        item(&g, "stone"),
        item(&g, "coal"),
        item(&g, "iron_ore"),
        item(&g, "planks_oak"),
    );
    let inv = g.session_mut().inventory_mut().unwrap();
    inv.set_slot(0, Some(ItemStack::new(stone, 8)));
    inv.set_slot(1, Some(ItemStack::new(coal, 3)));
    inv.set_slot(2, Some(ItemStack::new(ore, 2)));
    inv.set_slot(3, Some(ItemStack::new(plank, 2)));

    g.open_station(Station::Workbench);
    assert_eq!(g.state(), GameState::Inventory);
    let f = g.draw_ui();
    assert!(uses(&f, UiTexture::Workbench));
    assert!(!uses(&f, UiTexture::Inventory));
    let s = Station::Workbench;
    left(&mut g, at(s, 0));
    for k in [0, 1, 2, 3, 5, 6, 7, 8] {
        right(&mut g, at(s, nb_ecs::GRID_BASE + k));
    }
    assert!(g.inventory_cursor().held.is_none());
    left(&mut g, at(s, 1));
    right(&mut g, at(s, nb_ecs::GRID_BASE + 4)); // un charbon au centre
    left(&mut g, at(s, 1)); // repose les 2 autres
    let forge = item(&g, "forge");
    assert_eq!(g.craft_grid().output(), Some(ItemStack::new(forge, 1)));
    left(&mut g, at(s, nb_ecs::OUTPUT_SLOT));
    assert_eq!(g.inventory_cursor().held, Some(ItemStack::new(forge, 1)));
    assert!(g.craft_grid().is_empty());
    left(&mut g, at(s, 10)); // range la fonderie
    press(&mut g, keys::KEY_ESCAPE);
    assert_eq!(count_of(&g, "forge"), 1);
    assert_eq!(count_of(&g, "stone"), 0);
    assert_eq!(count_of(&g, "coal"), 2);

    // Fonderie : minerai + planche → lingot, deux fois (Maj sur le résultat).
    g.open_station(Station::Forge);
    assert!(uses(&g.draw_ui(), UiTexture::Forge));
    let s = Station::Forge;
    left(&mut g, at(s, 2));
    right(&mut g, at(s, nb_ecs::GRID_BASE));
    right(&mut g, at(s, nb_ecs::GRID_BASE));
    left(&mut g, at(s, 3));
    right(&mut g, at(s, nb_ecs::GRID_BASE + 1));
    right(&mut g, at(s, nb_ecs::GRID_BASE + 1));
    // Rien ne tient plus en main ; 2 minerais + 2 planches sont dans la grille.
    click_at(
        &mut g,
        at(s, nb_ecs::OUTPUT_SLOT),
        glfw::MOUSE_BUTTON_LEFT,
        true,
    );
    assert_eq!(count_of(&g, "iron_ingot"), 2);
    assert_eq!(count_of(&g, "iron_ore"), 0);
    assert_eq!(count_of(&g, "planks_oak"), 0);
    g.shutdown();
}

#[test]
fn right_click_on_a_placed_workbench_opens_it() {
    let dir = TempDir::new("world");
    let mut g = game(&dir);
    g.command(new_world("World"));
    let pid = g.session().player().unwrap();
    let pos = g
        .session()
        .sim
        .entities
        .copy::<nb_ecs::Transform>(pid)
        .unwrap()
        .pos;
    let (bx, bz) = (pos.x.floor() as i32, pos.z.floor() as i32);
    let t = Instant::now();
    while !g.world().is_chunk_loaded(bx, bz) {
        assert!(t.elapsed() < Duration::from_secs(60), "chunk jamais chargé");
        g.update(TIME_STEP);
        std::thread::sleep(Duration::from_millis(2));
    }
    // Un établi juste sous les pieds, regard vers le bas.
    let wb = g.regs.blocks.get("workbench").id;
    let by = pos.y.floor() as i32 - 1;
    g.world().set_block(bx, by, bz, wb);
    g.session_mut().update_camera(|c| c.pitch_deg = 89.0);
    g.input.mouse_button(glfw::MOUSE_BUTTON_RIGHT, true);
    tick(&mut g);
    g.input.mouse_button(glfw::MOUSE_BUTTON_RIGHT, false);
    assert_eq!(g.state(), GameState::Inventory, "clic droit sur l'établi");
    assert_eq!(g.craft_station(), Station::Workbench);
    assert!(uses(&g.draw_ui(), UiTexture::Workbench));
    // Fermer puis ouvrir l'inventaire normal : retour à la grille 2×2 de la main.
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Playing);
    press(&mut g, keys::KEY_E);
    assert_eq!(g.craft_station(), Station::Hand);
    assert_eq!(g.craft_grid().len(), 4);
    g.shutdown();
}

#[test]
fn pure_items_survive_save_and_reload() {
    let dir = TempDir::new("save");
    {
        let mut g = game(&dir);
        g.command(UiCommand::SwitchState(GameState::WorldSelect));
        g.command(new_world("Save"));
        let (stick, ingot, forge) = (item(&g, "stick"), item(&g, "iron_ingot"), item(&g, "forge"));
        let inv = g.session_mut().inventory_mut().unwrap();
        inv.set_slot(27, Some(ItemStack::new(stick, 17)));
        inv.set_slot(5, Some(ItemStack::new(ingot, 64)));
        inv.set_slot(6, Some(ItemStack::new(forge, 2)));
        press(&mut g, keys::KEY_ESCAPE);
        g.command(UiCommand::SaveAndMainMenu);
        assert!(g.take_error().is_none());
        g.shutdown();
    }
    let mut g = game(&dir);
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    let meta = g.worlds()[0].clone();
    assert!(
        meta.inventory_slots
            .iter()
            .any(|s| s.block_name == "stick" && s.count == 17),
        "objet pur sauvegardé par son nom"
    );
    g.command(UiCommand::PlayWorld(Box::new(meta)));
    let inv = g.session().inventory().unwrap();
    assert_eq!(inv.slot(27), Some(ItemStack::new(item(&g, "stick"), 17)));
    assert_eq!(
        inv.slot(5),
        Some(ItemStack::new(item(&g, "iron_ingot"), 64))
    );
    assert_eq!(inv.slot(6), Some(ItemStack::new(item(&g, "forge"), 2)));
    g.shutdown();
}

#[test]
fn unknown_item_names_in_a_save_are_skipped() {
    let dir = TempDir::new("unknown");
    {
        let mut g = game(&dir);
        g.command(UiCommand::SwitchState(GameState::WorldSelect));
        g.command(new_world("Unknown"));
        press(&mut g, keys::KEY_ESCAPE);
        g.command(UiCommand::SaveAndMainMenu);
        g.shutdown();
    }
    let mut g = game(&dir);
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    let mut meta = g.worlds()[0].clone();
    meta.inventory_slots.push(nb_world::save::InventorySlot {
        slot: 4,
        block_name: "objet_d_une_autre_version".into(),
        count: 3,
    });
    g.command(UiCommand::PlayWorld(Box::new(meta)));
    assert!(g.session().inventory().unwrap().slot(4).is_none());
    g.shutdown();
}

#[test]
fn slot_numbering_matches_between_ui_and_logic() {
    assert_eq!(nb_ui::inventory::GRID_BASE, nb_ecs::GRID_BASE);
    assert_eq!(nb_ui::inventory::OUTPUT_SLOT, nb_ecs::OUTPUT_SLOT);
}
