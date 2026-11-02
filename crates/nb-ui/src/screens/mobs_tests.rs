use super::*;

struct Harness {
    s: MobSettingsScreen,
    reg: MobRegistry,
    b: TypingBuffers,
    ctx: UiContext,
}

impl Harness {
    fn new() -> Self {
        let mut s = MobSettingsScreen::default();
        s.enter();
        Self {
            s,
            reg: MobRegistry::builtin(),
            b: TypingBuffers::default(),
            ctx: UiContext::default(),
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

    fn frame(&mut self, i: UiInput) -> Option<MobAction> {
        self.s.update(&i, &mut self.reg, &mut self.b, &mut self.ctx)
    }

    fn click(&mut self, x: f64, y: f64) -> Option<MobAction> {
        let r = self.frame(Self::input(x, y, true));
        self.frame(Self::input(x, y, false));
        r
    }

    fn esc(&mut self) -> Option<MobAction> {
        let mut i = Self::input(0.0, 0.0, false);
        i.esc_down = true;
        let r = self.frame(i);
        i.esc_down = false;
        self.frame(i);
        r
    }
}

// contentX = 240 ; row1 = 602, row2 = 660 ; liste à partir de y = 176 + 8.
const ROW1: f64 = 610.0;
const ROW2: f64 = 670.0;
const RIGHT: f64 = 700.0;

#[test]
fn rows_follow_behavior_and_medium() {
    let reg = MobRegistry::builtin();
    let reindeer = adv_rows(&reg.all()[0]);
    assert!(reindeer.iter().any(|r| r.label == "Herd Radius"));
    assert!(!reindeer.iter().any(|r| r.label == "Spawn Water Depth"));
    let fish = reg.get("fish").unwrap();
    assert_eq!(fish.medium, MobMedium::Water);
    assert!(adv_rows(fish)
        .iter()
        .any(|r| r.label == "Spawn Water Depth"));
    let mut aggressive = reg.all()[0].clone();
    aggressive.behavior_type = MobBehaviorType::Aggressive;
    let rows = adv_rows(&aggressive);
    assert!(rows.iter().any(|r| r.label == "Knockback Force"));
    assert!(!rows.iter().any(|r| r.label == "Herding"));
}

#[test]
fn value_display_rules() {
    let row = |v: f32, min: f32, max: f32| value_display(&AdvRow::slider("x", min, max, v));
    assert_eq!(row(10.0, 0.0, 100.0), "10");
    assert_eq!(row(10.004, 0.0, 100.0), "10");
    assert_eq!(row(0.55, 0.0, 1.0), "0.550");
    assert_eq!(row(2.25, 0.0, 10.0), "2.3"); // HALF_UP Java
    assert_eq!(
        value_display(&AdvRow::cycle("c", &["LAND", "WATER"], 1)),
        "WATER"
    );
}

#[test]
fn edit_and_save() {
    let mut h = Harness::new();
    // Ligne 0 à y = 184.
    h.click(300.0, 200.0);
    assert_eq!(h.s.selected, Some(0));
    h.click(300.0, ROW1);
    assert_eq!(h.s.screen, Screen::Edit);
    let old_name = h.reg.all()[0].display_name.clone();
    assert_eq!(h.b.command, old_name);
    assert!(h.b.typing_mode);
    h.b.command = "Caribou".into();
    // Max Health : en-tête 152..182, ligne 197, curseur 201..233 ; x 780..1040 → maximum.
    h.frame(Harness::input(790.0, 210.0, true));
    h.frame(Harness::input(5000.0, 210.0, true));
    h.frame(Harness::input(5000.0, 210.0, false));
    assert_eq!(h.click(300.0, ROW1), Some(MobAction::Saved));
    assert_eq!(h.s.screen, Screen::Main);
    let m = &h.reg.all()[0];
    assert_eq!(m.display_name, "Caribou");
    assert_eq!(m.max_health, 100.0);
    assert_eq!(m.id, "reindeer");
    assert!(m.geometry.is_some(), "géométrie conservée");
    assert!(!h.b.typing_mode);
}

#[test]
fn cycle_behavior_rebuilds_rows() {
    let mut h = Harness::new();
    h.click(300.0, 200.0);
    h.click(300.0, ROW1);
    // Replier « Physical » (en-tête y = 152) pour faire remonter la suite.
    h.click(300.0, 160.0);
    assert_eq!(h.s.adv.rows[0].kind, AdvKind::Header { collapsed: true });
    let before = h.s.adv.rows.len();
    let idx =
        h.s.adv
            .rows
            .iter()
            .position(|r| r.label == "Behavior Type")
            .unwrap();
    // Ouvre la ligne via un clic logique : cycle direct puis reconstruction.
    if let AdvKind::Cycle { index, .. } = &mut h.s.adv.rows[idx].kind {
        *index = 1;
    }
    h.s.rebuild();
    let rows = &h.s.adv.rows;
    assert_ne!(rows.len(), before);
    assert!(rows.iter().any(|r| r.label == "Attack Damage"));
    assert_eq!(
        rows[0].kind,
        AdvKind::Header { collapsed: true },
        "repli conservé"
    );
    let c = h.s.editing.as_ref().unwrap();
    assert_eq!(c.behavior_type, MobBehaviorType::Aggressive);
}

#[test]
fn aggressive_defaults_applied_once() {
    let mut c = MobRegistry::builtin().all()[0].clone();
    c.chase_speed = 0.0;
    c.attack_range = 0.0;
    let rows = vec![AdvRow::cycle(
        "Behavior Type",
        &["PASSIVE", "AGGRESSIVE"],
        1,
    )];
    apply_to_config(&rows, &mut c);
    assert_eq!((c.chase_speed, c.attack_cooldown), (2.0, 1.5));
}

#[test]
fn add_mob() {
    let mut h = Harness::new();
    h.click(RIGHT, ROW1);
    assert_eq!(h.s.screen, Screen::Add);
    assert_eq!(h.s.editing.as_ref().unwrap().aggressor_detect_range, 10.0);
    assert_eq!(h.click(300.0, ROW1), None, "identifiant vide");
    h.b.command = "fish".into();
    // Champ nom (y 172..216) → tampon secondaire.
    h.click(300.0, 180.0);
    assert!(h.b.use_secondary);
    h.b.secondary = "Shark".into();
    assert_eq!(h.click(300.0, ROW1), None, "identifiant déjà pris");
    h.b.command = "bad id".into();
    assert_eq!(h.click(300.0, ROW1), None, "caractère interdit");
    h.b.command = "shark_2".into();
    assert_eq!(
        h.click(300.0, ROW1),
        Some(MobAction::Added("shark_2".into()))
    );
    let m = h.reg.get("shark_2").unwrap();
    assert_eq!(m.display_name, "Shark");
    assert!(h.b.secondary.is_empty());
}

#[test]
fn delete_reset_and_escape() {
    let mut h = Harness::new();
    h.click(300.0, 200.0 + 88.0);
    h.click(300.0, ROW2);
    assert_eq!(h.s.screen, Screen::Delete);
    let mut dl = DrawList::new(1280, 720);
    h.s.draw(&mut dl, &Harness::input(0.0, 0.0, false), &h.reg, &h.b);
    assert!(dl.vertices().iter().any(|v| v.color == YELLOW));
    assert_eq!(h.click(300.0, ROW1), Some(MobAction::Saved));
    assert!(h.reg.get("fish").is_none());
    // Reset (x 496..784, y 132..168) puis confirmation.
    h.click(640.0, 140.0);
    assert_eq!(h.s.screen, Screen::ResetConfirm);
    assert_eq!(h.esc(), None);
    assert_eq!(h.s.screen, Screen::Main);
    h.click(640.0, 140.0);
    assert_eq!(h.click(300.0, ROW1), Some(MobAction::Reset));
    assert_eq!(h.reg.all().len(), 3);
    assert_eq!(h.esc(), Some(MobAction::Leave));
    assert_eq!(h.click(RIGHT, ROW2), Some(MobAction::Leave));
}

#[test]
fn tabs() {
    let mut h = Harness::new();
    assert_eq!(
        h.click(348.0 + 148.0 + 1.0, 80.0),
        Some(MobAction::SwitchTo(GameState::Settings))
    );
    assert!(h.ctx.pending_controls_tab);
    assert_eq!(
        h.click(348.0 + 296.0 + 1.0, 80.0),
        Some(MobAction::SwitchTo(GameState::ResourcePacks))
    );
    assert_eq!(h.click(348.0 + 444.0 + 1.0, 80.0), None, "onglet courant");
}

#[test]
fn main_draw_lists_rows() {
    let h = Harness::new();
    let mut dl = DrawList::new(1280, 720);
    h.s.draw(&mut dl, &Harness::input(0.0, 0.0, false), &h.reg, &h.b);
    let rows = dl
        .vertices()
        .iter()
        .filter(|v| v.color == [0.15, 0.15, 0.15, 0.9])
        .count();
    assert_eq!(rows, 3 * 6);
}
