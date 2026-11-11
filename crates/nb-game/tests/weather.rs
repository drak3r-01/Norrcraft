//! Météo dans le jeu (plan Norrcraft, session 3.3). Aucun GPU.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_data::{keys, WorldKind};
use nb_game::input::glfw;
use nb_game::{
    Game, GameMode, GameOptions, NewWorld, PlatformRequest, Registries, UiCommand, TIME_STEP,
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
        let p = std::env::temp_dir().join(format!("nb-wea-{tag}-{}-{nanos}", std::process::id()));
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

fn press(g: &mut Game, code: i32) {
    g.input.key_event(code, true, false);
    g.update(TIME_STEP);
    g.input.key_event(code, false, false);
    g.update(TIME_STEP);
}

fn say(g: &mut Game, text: &str) {
    press(g, glfw::KEY_T);
    for c in text.chars() {
        g.input.char_event(c);
    }
    press(g, keys::KEY_ENTER);
}

fn last_reply(g: &Game) -> String {
    g.chat()
        .log()
        .last()
        .map(|m| m.text.clone())
        .unwrap_or_default()
}

#[test]
fn norrcraft_weather_varies_and_the_command_forces_it() {
    let d = TempDir::new("norr");
    let mut g = game(&d, 31_000 + (std::process::id() % 3_000) as u16);
    g.command(new_world("Vaeder", WorldKind::Norrcraft));
    let start = Instant::now();
    while g.session().environment.is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "météo jamais calculée"
        );
        tick(&mut g);
        std::thread::sleep(Duration::from_millis(4));
    }
    let env = g.session().environment.unwrap();
    assert!((0.0..=1.0).contains(&env.wind.speed));

    // Sur deux ans, le ciel passe du dégagé au couvert.
    let (mut lo, mut hi) = (f32::MAX, 0.0f32);
    let mut rain_or_snow = false;
    for day in 0..48 {
        for tod in [100.0, 400.0, 700.0, 900.0] {
            g.sky_mut().day = day;
            g.sky_mut().set_time(tod);
            tick(&mut g);
            let w = g.sky().weather;
            lo = lo.min(w.overcast);
            hi = hi.max(w.overcast);
            let env = g.session().environment.unwrap();
            rain_or_snow |= env.rain > 0.1 || env.snow > 0.1;
        }
    }
    assert!(lo < 0.2 && hi > 0.6, "couverture {lo}..{hi}");
    assert!(rain_or_snow, "jamais de précipitations en deux ans");

    // `/weather` : informations, état imposé, retour à l'automatique.
    say(&mut g, "weather");
    assert!(last_reply(&g).starts_with("Météo : "), "{}", last_reply(&g));
    say(&mut g, "weather blizzard");
    assert!(last_reply(&g).contains("blizzard"), "{}", last_reply(&g));
    tick(&mut g);
    let env = g.session().environment.unwrap();
    assert_eq!(env.chill, 3.0);
    assert_eq!(env.snow, 1.0);
    assert!(g.sky().weather.fog > 0.1, "visibilité réduite");
    assert!(g.sky().weather.overcast > 0.9);
    say(&mut g, "weather nimportequoi");
    assert!(last_reply(&g).starts_with("Usage"), "{}", last_reply(&g));
    say(&mut g, "weather auto");
    assert!(last_reply(&g).contains("automatique"));
    tick(&mut g);
    assert_ne!(g.session().environment.unwrap().chill, 3.0 + 1.0);
    g.shutdown();
}

#[test]
fn vanilla_world_has_no_weather_model() {
    let d = TempDir::new("vanilla");
    let mut g = game(&d, 34_000 + (std::process::id() % 3_000) as u16);
    g.command(new_world("Classique", WorldKind::Vanilla));
    for _ in 0..30 {
        tick(&mut g);
    }
    assert!(g.session().environment.is_none());
    assert_eq!(g.sky().weather, nb_gfx::SkyWeather::default());
    // La commande d'origine répond toujours.
    say(&mut g, "weather rain");
    assert!(last_reply(&g).contains("pluie"), "{}", last_reply(&g));
    assert_eq!(g.fx().weather(), (1.0, 0.0));
    g.shutdown();
}
