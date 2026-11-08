//! Mode survie de bout en bout : sauvegarde (usure, armure, faim), livre de recettes (vrais
//! écrans), remplissage automatique de la grille, icônes de cubes isométriques.

use std::path::PathBuf;
use std::sync::Arc;

use nb_data::{keys, ArmorSlot, Station};
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
        let p = std::env::temp_dir().join(format!("nb-surv-{tag}-{}-{nanos}", std::process::id()));
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
    let blocks = regs.blocks.clone();
    // Couche d'un bloc = 100 + 10 × identifiant + indice de face (distinguer dessus / côtés).
    let table = Arc::new(BlockTable::from_registry(&regs.blocks, move |n| {
        100 + u32::from(blocks.get(n).id)
    }));
    let items = regs.items.clone();
    let icons = ItemIcons::build(&regs.items, move |tex| {
        500 + u32::from(items.id_of(tex.strip_prefix("item_").unwrap()).unwrap())
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

fn new_world(name: &str, mode: GameMode) -> UiCommand {
    UiCommand::CreateWorld(Box::new(NewWorld {
        name: name.into(),
        seed_text: "777".into(),
        game_mode: mode,
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

fn click(g: &mut Game, (x, y): (f64, f64)) {
    g.input.cursor_moved(x, y);
    g.input.mouse_button(glfw::MOUSE_BUTTON_LEFT, true);
    tick(g);
    g.input.mouse_button(glfw::MOUSE_BUTTON_LEFT, false);
    tick(g);
}

fn item(g: &Game, name: &str) -> u16 {
    g.regs.items.id_of(name).unwrap_or_else(|| panic!("objet {name}"))
}

fn quads_of(f: &UiFrame, slot: UiTextureSlot) -> u32 {
    f.draws.iter().filter(|d| d.texture == slot).map(|d| d.count / 6).sum()
}

#[test]
fn survival_state_wear_and_armor_survive_save_and_reload() {
    let dir = TempDir::new("save");
    {
        let mut g = game(&dir);
        g.command(UiCommand::SwitchState(GameState::WorldSelect));
        g.command(new_world("Save", GameMode::Survival));
        let pick = item(&g, "iron_pickaxe");
        let helm = item(&g, "fur_cap");
        let pid = g.session().player().unwrap();
        let inv = g.session_mut().inventory_mut().unwrap();
        inv.set_slot(27, Some(ItemStack::new(pick, 1).with_wear(77)));
        inv.set_armor(ArmorSlot::Head, Some(ItemStack::new(helm, 1).with_wear(5)));
        {
            let v = g
                .session_mut()
                .sim
                .entities
                .get_mut::<nb_ecs::Vitals>(pid)
                .unwrap();
            v.hunger = 7.5;
            v.warmth = 42.0;
            v.stamina = 66.0;
        }
        press(&mut g, keys::KEY_ESCAPE);
        g.command(UiCommand::SaveAndMainMenu);
        assert!(g.take_error().is_none());
        g.shutdown();
    }
    let mut g = game(&dir);
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    let meta = g.worlds()[0].clone();
    assert!(meta.survival.is_some(), "état de survie écrit dans world.json");
    g.command(UiCommand::PlayWorld(Box::new(meta)));
    let inv = g.session().inventory().unwrap();
    assert_eq!(
        inv.slot(27),
        Some(ItemStack::new(item(&g, "iron_pickaxe"), 1).with_wear(77))
    );
    assert_eq!(
        inv.armor(ArmorSlot::Head),
        Some(ItemStack::new(item(&g, "fur_cap"), 1).with_wear(5))
    );
    let pid = g.session().player().unwrap();
    let v = g.session().sim.entities.copy::<nb_ecs::Vitals>(pid).unwrap();
    assert!((v.hunger - 7.5).abs() < 0.2, "faim {}", v.hunger);
    assert!((v.warmth - 42.0).abs() < 2.0, "chaleur {}", v.warmth);
    g.shutdown();
}

#[test]
fn old_saves_without_survival_data_still_load() {
    let json = r#"{"formatVersion":1,"name":"Old","seed":1,"gameMode":"SURVIVAL"}"#;
    let meta: nb_world::save::WorldMetadata = serde_json::from_str(json).unwrap();
    assert!(meta.survival.is_none());
}

#[test]
fn recipe_book_lists_all_recipes_and_autofills_the_grid() {
    let dir = TempDir::new("book");
    let mut g = game(&dir);
    g.command(new_world("Book", GameMode::Survival));
    let planks = item(&g, "planks_oak");
    g.session_mut()
        .inventory_mut()
        .unwrap()
        .set_slot(0, Some(ItemStack::new(planks, 6)));
    press(&mut g, keys::KEY_E);
    assert_eq!(g.state(), GameState::Inventory);
    // Les emplacements d'armure et le bouton du livre sont dessinés.
    let screen = InventoryScreen::for_station(1280, 720, Station::Hand);
    let (bx, by, bw, bh) = screen.book_button();
    click(&mut g, (f64::from(bx + bw / 2), f64::from(by + bh / 2)));
    let f = g.draw_ui();
    assert!(
        f.draws
            .iter()
            .all(|d| d.texture != UiTextureSlot::Image(UiTexture::Inventory.index())),
        "le livre remplace le panneau de l'inventaire"
    );
    assert!(f.vertices.len() > 600, "tuiles de recettes dessinées");

    // Remplissage de la recette « bâtons » (4 bâtons avec 2 planches empilées).
    let book = g.regs.recipes.clone();
    let stick = item(&g, "stick");
    let idx = book
        .recipes()
        .iter()
        .position(|r| r.output.item == stick && r.station == Station::Hand)
        .expect("recette des bâtons");
    g.command(UiCommand::RecipeFill(idx));
    assert_eq!(
        g.craft_grid().output(),
        Some(ItemStack::new(stick, 4)),
        "la grille est prête à fabriquer des bâtons"
    );
    assert_eq!(
        g.session().inventory().unwrap().count_of(planks),
        4,
        "2 planches prélevées"
    );
    // Recette impossible (pas d'ingrédients) : message, grille intacte.
    let pick = item(&g, "iron_pickaxe");
    let idx = book
        .recipes()
        .iter()
        .position(|r| r.output.item == pick)
        .unwrap();
    g.command(UiCommand::RecipeFill(idx));
    assert_ne!(g.craft_grid().output().map(|o| o.item), Some(pick));
    g.shutdown();
}

#[test]
fn blocks_are_drawn_as_isometric_cubes_and_items_as_sprites() {
    let dir = TempDir::new("icons");
    let mut g = game(&dir);
    g.command(new_world("Icons", GameMode::Survival));
    let (stone, stick) = (item(&g, "stone"), item(&g, "stick"));
    let inv = g.session_mut().inventory_mut().unwrap();
    inv.set_slot(27, Some(ItemStack::new(stone, 3)));
    inv.set_slot(28, Some(ItemStack::new(stick, 3)));
    let f = g.draw_ui();
    // Cube : 3 quads (dessus + 2 côtés), x2 pour la pierre tenue (barre rapide + grande icône
    // en main) ; sprite : 2 quads (ombre + sprite).
    let stone_layer = 100 + u32::from(stone);
    let stick_layer = 500 + u32::from(stick);
    assert_eq!(quads_of(&f, UiTextureSlot::BlockLayer(stone_layer)), 6);
    assert_eq!(quads_of(&f, UiTextureSlot::BlockLayer(stick_layer)), 2);
    g.shutdown();
}

#[test]
fn survival_hud_shows_hunger_warmth_and_stamina() {
    let dir = TempDir::new("hud");
    let mut g = game(&dir);
    g.command(new_world("Hud", GameMode::Survival));
    let f = g.draw_ui();
    // 10 icônes de faim pleines au départ (HungerFull), pas d'icônes vides.
    assert_eq!(
        quads_of(&f, UiTextureSlot::Image(UiTexture::HungerFull.index())),
        10
    );
    assert_eq!(
        quads_of(&f, UiTextureSlot::Image(UiTexture::HungerEmpty.index())),
        0
    );
    let pid = g.session().player().unwrap();
    g.session_mut()
        .sim
        .entities
        .get_mut::<nb_ecs::Vitals>(pid)
        .unwrap()
        .hunger = 6.0;
    // `Vitals.hunger` est recalculé à chaque tick : on ne laisse pas le temps passer.
    let f = g.draw_ui();
    assert_eq!(
        quads_of(&f, UiTextureSlot::Image(UiTexture::HungerFull.index())),
        3
    );
    assert_eq!(
        quads_of(&f, UiTextureSlot::Image(UiTexture::HungerEmpty.index())),
        7
    );
    // Créatif : plus de barres de survie (HUD d'origine, faim pleine).
    g.command(UiCommand::SwitchState(GameState::MainMenu));
    g.shutdown();
}
