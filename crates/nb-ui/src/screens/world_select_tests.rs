use super::*;

fn defaults() -> WorldGenParams {
    WorldGenParams {
        sea_level: 34,
        base_height: 30,
        height_scale: 38,
        ocean_frequency: 0.0012,
        ocean_threshold: 0.38,
        height_noise_frequency: 0.01,
        height_noise_octaves: 5,
        height_noise_lacunarity: 2.0,
        height_noise_gain: 0.45,
        river_width: 0.04,
        river_depth: 6,
        river_frequency: 0.002,
        temp_scale: 0.002,
        humid_scale: 0.002,
        mountain_frequency: 0.003,
        snow_line_mountains: 70,
        snow_line_taiga: 60,
        vegetation_density: 1.0,
        tree_density_overrides: [-1.0; 8],
        caves_enabled: true,
        cave_threshold: 0.5,
        cavern_threshold: 0.7,
        cave_depth_max: 60,
        coal_chance: 0.012,
        coal_min_y: 5,
        coal_max_y: 80,
        iron_chance: 0.008,
        iron_min_y: 5,
        iron_max_y: 60,
        gold_chance: 0.004,
        gold_min_y: 5,
        gold_max_y: 32,
        diamond_chance: 0.002,
        diamond_min_y: 5,
        diamond_max_y: 16,
        emerald_chance: 0.001,
        emerald_min_y: 5,
        emerald_max_y: 30,
    }
}

fn flat(_: WorldType) -> WorldGenParams {
    WorldGenParams {
        sea_level: 4,
        ..defaults()
    }
}

fn worlds() -> Vec<WorldEntry> {
    vec![
        WorldEntry {
            name: "Alpha".into(),
            last_played_at: 1_700_000_000_000,
            game_mode: "SURVIVAL".into(),
            allow_cheats: false,
        },
        WorldEntry {
            name: "Beta".into(),
            last_played_at: 0,
            game_mode: String::new(),
            allow_cheats: true,
        },
    ]
}

fn exists(n: &str) -> bool {
    n == "Alpha" || n == "Beta"
}

struct Harness {
    s: WorldSelectScreen,
    b: TypingBuffers,
}

impl Harness {
    fn new() -> Self {
        let mut s = WorldSelectScreen::new(defaults());
        s.enter(worlds());
        Self {
            s,
            b: TypingBuffers::default(),
        }
    }

    fn input(x: f64, y: f64, down: bool) -> UiInput {
        UiInput {
            width: 1280,
            height: 720,
            mouse_x: x,
            mouse_y: y,
            mouse_down: down,
            dt: 0.1,
            ..Default::default()
        }
    }

    fn frame(&mut self, x: f64, y: f64, down: bool) -> Option<WorldAction> {
        self.s
            .update(&Self::input(x, y, down), &mut self.b, &exists, &flat)
    }

    fn click(&mut self, x: f64, y: f64) -> Option<WorldAction> {
        let r = self.frame(x, y, true);
        self.frame(x, y, false);
        r
    }
}

// contentX = 240 ; row1 = 602, row2 = 660 ; gros boutons 395, petits 192.
const ROW1: f64 = 610.0;
const ROW2: f64 = 670.0;

#[test]
fn dates() {
    assert_eq!(format_date(0), "1970-01-01");
    assert_eq!(format_date(1_700_000_000_000), "2023-11-14");
    assert_eq!(format_date(951_782_400_000), "2000-02-29");
    assert_eq!(format_date(-1), "1969-12-31");
}

#[test]
fn select_play_and_disabled_buttons() {
    let mut h = Harness::new();
    assert_eq!(
        h.click(300.0, ROW1),
        None,
        "aucune sélection : Play inactif"
    );
    // Ligne 1 : y = 64 + 8 + 88 = 160.
    h.click(300.0, 170.0);
    assert_eq!(h.s.selected, Some(1));
    assert_eq!(h.click(300.0, ROW1), Some(WorldAction::Play("Beta".into())));
    assert_eq!(h.click(1030.0, ROW2), Some(WorldAction::MainMenu));
}

#[test]
fn edit_flow() {
    let mut h = Harness::new();
    h.click(300.0, 80.0);
    h.click(300.0, ROW2);
    assert_eq!(h.s.screen, Screen::Edit);
    assert_eq!(h.b.command, "Alpha");
    assert!(h.b.typing_mode && h.b.suppress_chat_filter);
    assert_eq!(h.s.edit_game_mode, "SURVIVAL");
    // midY = 64 + 526/3 = 239 ; input 269 ; mode de jeu 345 ; triche 405.
    h.click(1000.0, 350.0);
    assert_eq!(h.s.edit_game_mode, "CREATIVE");
    h.click(1020.0, 410.0);
    assert!(h.s.edit_allow_cheats);
    // Renommer vers un nom existant (autre que l'ancien) : refusé.
    h.b.command = "Beta".into();
    assert_eq!(h.click(300.0, ROW1), None);
    h.b.command = " Gamma ".into();
    assert_eq!(
        h.click(300.0, ROW1),
        Some(WorldAction::Edit {
            old_name: "Alpha".into(),
            new_name: "Gamma".into(),
            game_mode: "CREATIVE".into(),
            allow_cheats: true
        })
    );
    assert_eq!(h.s.screen, Screen::Main);
    assert!(!h.b.typing_mode);
}

#[test]
fn delete_and_recreate() {
    let mut h = Harness::new();
    h.click(300.0, 80.0);
    h.click(240.0 + 202.0 + 5.0, ROW2);
    assert_eq!(h.s.screen, Screen::Delete);
    assert_eq!(h.s.selected, None);
    assert_eq!(
        h.click(300.0, ROW1),
        Some(WorldAction::Delete("Alpha".into()))
    );
    h.s.set_worlds(worlds()[1..].to_vec());
    h.click(300.0, 80.0);
    h.click(240.0 + 405.0 + 5.0, ROW2);
    assert_eq!(h.s.screen, Screen::Recreate);
    assert!(h.b.command.is_empty());
    assert_eq!(h.click(300.0, ROW1), None, "nom vide");
    h.b.command = "Copy".into();
    assert_eq!(
        h.click(300.0, ROW1),
        Some(WorldAction::Recreate {
            source: "Beta".into(),
            new_name: "Copy".into()
        })
    );
}

#[test]
fn create_flow_keeps_java_seed_bug() {
    let mut h = Harness::new();
    h.click(700.0, ROW1);
    assert_eq!(h.s.screen, Screen::Create);
    assert_eq!(h.b.command, "My World");
    // Champ germe (y 172..216) puis saisie.
    h.click(300.0, 180.0);
    assert!(h.b.use_secondary);
    h.b.on_char('4');
    h.b.on_char('2');
    // Type de monde → préréglage (FLAT).
    h.click(900.0, 330.0);
    assert_eq!(h.s.create_world_type, WorldType::Flat);
    assert_eq!(h.s.pending.sea_level, 4);
    h.click(950.0, 235.0);
    assert_eq!(h.s.create_game_mode, "CREATIVE");
    let r = h.click(300.0, ROW1);
    let Some(WorldAction::Create {
        name,
        seed_text,
        world_type,
        params,
        game_mode,
        ..
    }) = r
    else {
        panic!("création attendue : {r:?}");
    };
    assert_eq!(name, "My World");
    assert_eq!(seed_text, "42", "germe saisi conservé (correctif par défaut)");
    assert_eq!(world_type, WorldType::Flat);
    assert_eq!(game_mode, "CREATIVE");
    assert_eq!(params.sea_level, 4);
    // Arrondi float → double des curseurs conservé (applyAdvToConfig).
    assert_eq!(params.ocean_threshold, 0.38f32 as f64);
}

#[test]
fn seed_fix_flag() {
    let mut h = Harness::new();
    assert!(h.s.fix_seed_input, "correctif actif par défaut");
    h.click(700.0, ROW1);
    h.b.secondary = "1234".into();
    let Some(WorldAction::Create { seed_text, .. }) = h.click(300.0, ROW1) else {
        panic!("création attendue")
    };
    assert_eq!(seed_text, "1234");
    // Comportement Java d'origine : le germe est vidé avant d'être lu.
    let mut h = Harness::new();
    h.s.fix_seed_input = false;
    h.click(700.0, ROW1);
    h.b.secondary = "1234".into();
    let Some(WorldAction::Create { seed_text, .. }) = h.click(300.0, ROW1) else {
        panic!("création attendue")
    };
    assert_eq!(seed_text, "", "[JAVA-BUG] germe effacé avant lecture");
}

#[test]
fn random_seed_button() {
    let mut h = Harness::new();
    h.click(700.0, ROW1);
    h.click(240.0 + 750.0, 180.0);
    assert!(h.b.secondary.parse::<i64>().unwrap() >= 0);
    assert!(!h.b.use_secondary);
}

#[test]
fn advanced_panel() {
    let mut h = Harness::new();
    h.click(700.0, ROW1);
    h.click(300.0, 390.0);
    assert!(h.s.advanced_expanded);
    // Tout replié : 4 + 7 en-têtes × 45.
    assert_eq!(h.s.adv.content_height(), 4 + 7 * 45);
    // En-tête « Terrain » à y = 428.
    h.click(300.0, 435.0);
    assert_eq!(h.s.adv.rows[0].kind, AdvKind::Header { collapsed: false });
    // Sea Level : ligne 473, curseur 477..509, x 780..1040 → glisser au maximum.
    h.frame(790.0, 490.0, true);
    assert!(h.s.adv.active_slider.is_some());
    h.frame(2000.0, 490.0, true);
    assert_eq!(h.s.pending.sea_level, 80);
    h.frame(2000.0, 490.0, false);
    assert!(h.s.adv.active_slider.is_none());
    // Le repli survit au changement de type de monde.
    h.click(900.0, 330.0);
    assert_eq!(h.s.adv.rows[0].kind, AdvKind::Header { collapsed: false });
    let mut dl = DrawList::new(1280, 720);
    h.s.draw(&mut dl, &Harness::input(0.0, 0.0, false), &h.b, &exists);
    assert!(dl.batches().iter().any(|b| b.scissor.is_some()));
}

#[test]
fn value_display_rules() {
    let rows = adv_rows(&defaults());
    let get = |l: &str| value_display(rows.iter().find(|r| r.label == l).unwrap());
    assert_eq!(get("Sea Level"), "34");
    assert_eq!(get("Ocean Coverage"), "0.380");
    // (float) -0.001 élargi en double : |v| > 0.001, Java affiche donc « -0.0010 » et non « Auto ».
    assert_eq!(get("Plains Trees"), "-0.0010");
    assert_eq!(get("Coal Chance"), "0.0120");
    assert_eq!(get("Lacunarity"), "2");
    assert_eq!(get("Vegetation Density"), "1");
    assert_eq!(get("Cave Generation"), "ON");
    assert_eq!(get("Continent Size: Small <-> Large"), "0.0012");
}

#[test]
fn apply_round_trip() {
    let d = defaults();
    let mut c = d.clone();
    apply_adv(&adv_rows(&d), &mut c);
    assert_eq!(c.sea_level, 34);
    assert_eq!(c.tree_density_overrides, [-1.0; 8]);
    assert!((c.ocean_frequency - 0.0012).abs() < 1e-9);
    assert_eq!(c.cave_depth_max, 60);
    assert!(c.caves_enabled);
}

#[test]
fn main_screen_draw() {
    let h = Harness::new();
    let mut dl = DrawList::new(1280, 720);
    h.s.draw(&mut dl, &Harness::input(0.0, 0.0, false), &h.b, &exists);
    let clip = Scissor {
        x: 0,
        y: 64,
        w: 1280,
        h: 526,
    };
    assert!(dl.batches().iter().any(|b| b.scissor == Some(clip)));
    // « Play Selected World » inactif (teinte 0,45).
    assert!(dl.vertices().iter().any(|v| v.color == gray(0.45)));
}
