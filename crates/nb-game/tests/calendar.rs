//! Calendrier et lumière du jour dans le jeu (plan Norrcraft, session 3.1). Aucun GPU.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_data::WorldKind;
use nb_game::{
    Game, GameMode, GameOptions, NewWorld, PlatformRequest, Registries, UiCommand, TIME_STEP,
};
use nb_mesh::BlockTable;
use nb_world::calendar::{Calendar, Season};
use nb_world::save;
use nb_world::WorldConfig;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-cal-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn game(dir: &TempDir, discovery_port: u16) -> Game {
    std::fs::write(dir.0.join("settings.json"), r#"{ "renderDistance": 2 }"#).unwrap();
    let regs = Registries::builtin();
    let table = Arc::new(BlockTable::from_registry(&regs.blocks, |_| 0));
    let mut o = GameOptions::new(&dir.0);
    o.menu_seed = Some(1234);
    o.rng_seed = Some(99);
    o.mesh_threads = Some(1);
    o.lan_port = 0;
    o.lan_discovery_port = Some(discovery_port);
    Game::new(o, regs, table)
}

fn tick(g: &mut Game) {
    g.update(TIME_STEP);
    g.input.end_frame();
    for r in g.take_requests() {
        if r == PlatformRequest::RebuildBlockTable {
            let table = Arc::new(BlockTable::from_registry(&g.regs.blocks, |_| 0));
            g.set_block_table(table);
        }
    }
}

fn new_world(name: &str, kind: WorldKind) -> UiCommand {
    UiCommand::CreateWorld(Box::new(NewWorld {
        name: name.into(),
        seed_text: "31337".into(),
        game_mode: GameMode::Creative,
        allow_cheats: true,
        config: WorldConfig::default(),
        world_kind: kind,
    }))
}

fn command(g: &mut Game, text: &str) -> Option<String> {
    let mut sky = *g.sky();
    let reply = g.session_mut().calendar_command(text, &mut sky);
    *g.sky_mut() = sky;
    reply
}

#[test]
fn norrcraft_world_has_a_calendar_that_drives_the_sky_and_is_saved() {
    let d = TempDir::new("norr");
    let mut g = game(&d, 26_000 + (std::process::id() % 3_000) as u16);
    g.command(new_world("Kalender", WorldKind::Norrcraft));
    let cal = g.session().calendar().expect("Norrcraft a des saisons");
    assert_eq!(cal.days_per_month(), 2);
    assert_eq!(g.sky().day, 0);

    // Quelques secondes de jeu : la latitude du lieu est relevée et lissée.
    let start = Instant::now();
    while g.sky().latitude <= 0.0 {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "latitude jamais relevée"
        );
        tick(&mut g);
        std::thread::sleep(Duration::from_millis(4));
    }
    let lat = g.sky().latitude;
    assert!((0.0..=1.0).contains(&lat), "{lat}");
    // Départ au printemps : le soleil se couche (pas de soleil de minuit) ou presque partout.
    assert_eq!(cal.date(g.sky().day).season, Season::Var);

    // Plein été à la latitude relevée : le décalage solaire suit la durée du jour.
    let n = u64::from(cal.days_per_year());
    let midsummer = (n / 2 + n - u64::from(cal.day_of_year(0))) % n;
    g.sky_mut().day = midsummer;
    g.sky_mut().set_time(0.0);
    tick(&mut g);
    let summer = g.sky().solar_bias;
    let midwinter = (n - u64::from(cal.day_of_year(0))) % n;
    g.sky_mut().day = midwinter;
    tick(&mut g);
    let winter = g.sky().solar_bias;
    assert!(summer > 0.0 && winter < 0.0, "été {summer}, hiver {winter}");
    assert!(summer > winter + 0.3);

    // Les commandes.
    let text = command(&mut g, "/almanach").unwrap();
    for needle in ["Almanach", "Jour", "Fête", "Lune", "Signes"] {
        assert!(text.contains(needle), "{needle} : {text}");
    }
    let r = command(&mut g, "/date set 13").unwrap();
    assert!(r.contains("jour 13/24"), "{r}");
    let r = command(&mut g, "/date add 5").unwrap();
    assert!(r.contains("jour 18/24"), "{r}");
    assert!(command(&mut g, "/date set").unwrap().starts_with("Usage"));
    assert!(command(&mut g, "/time set 3").is_none(), "pas pour nous");

    // Sauvegarde : le compteur de jours revient.
    let day = g.sky().day;
    g.save_current();
    let meta = save::load_metadata(&d.0, "Kalender").unwrap();
    assert_eq!(meta.world_config.calendar_day, day);
    g.command(UiCommand::SaveAndMainMenu);
    g.command(UiCommand::PlayWorld(Box::new(meta)));
    assert_eq!(g.sky().day, day);
    assert_eq!(g.session().calendar(), Some(Calendar::default()));
    g.shutdown();
}

#[test]
fn vanilla_world_keeps_the_original_sky() {
    let d = TempDir::new("vanilla");
    let mut g = game(&d, 29_000 + (std::process::id() % 3_000) as u16);
    g.command(new_world("Classique", WorldKind::Vanilla));
    assert!(g.session().calendar().is_none());
    for _ in 0..30 {
        tick(&mut g);
    }
    assert_eq!(g.sky().solar_bias, 0.0);
    assert_eq!(g.sky().latitude, 0.0);
    assert!(command(&mut g, "/almanach")
        .unwrap()
        .contains("pas de calendrier"));
    assert!(command(&mut g, "/date")
        .unwrap()
        .contains("pas de calendrier"));
    g.shutdown();
}
