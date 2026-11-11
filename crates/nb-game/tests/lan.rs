//! Partie LAN de bout en bout : deux `Game` dans le même processus, reliés par le réseau local
//! (boucle locale). Aucun GPU. Couvre : découverte, connexion, monde répliqué, blocs dans les
//! deux sens, joueurs distants, mobs, chat, départ d'un client, fermeture par l'hôte.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_data::keys;
use nb_game::input::glfw;
use nb_game::net::LanRole;
use nb_game::{Game, GameMode, GameOptions, GameState, NewWorld, Registries, UiCommand, TIME_STEP};
use nb_mesh::BlockTable;
use nb_world::WorldConfig;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-lan-{tag}-{}-{nanos}", std::process::id()));
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
}

fn press(g: &mut Game, code: i32) {
    g.input.key_event(code, true, false);
    tick(g);
    g.input.key_event(code, false, false);
    tick(g);
}

/// Fait tourner les deux jeux jusqu'à ce que `done` soit vrai (ou 30 s).
fn run_until(a: &mut Game, b: &mut Game, what: &str, mut done: impl FnMut(&Game, &Game) -> bool) {
    let start = Instant::now();
    while !done(a, b) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "délai dépassé : {what}\nhôte : {:?}\nclient : {:?}",
            a.lan_view(),
            b.lan_view()
        );
        tick(a);
        tick(b);
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn new_world(name: &str) -> UiCommand {
    UiCommand::CreateWorld(Box::new(NewWorld {
        name: name.into(),
        seed_text: "31337".into(),
        game_mode: GameMode::Creative,
        allow_cheats: true,
        config: WorldConfig::default(),
        world_kind: Default::default(),
    }))
}

/// Port TCP annoncé par l'hôte (`ip:port`).
fn host_port(a: &Game) -> u16 {
    let addr = a
        .lan_view()
        .addresses
        .first()
        .expect("adresse de l'hôte")
        .clone();
    addr.rsplit(':').next().unwrap().parse().unwrap()
}

fn type_chat(g: &mut Game, text: &str) {
    press(g, glfw::KEY_T);
    assert!(g.chat().is_active());
    for c in text.chars() {
        g.input.char_event(c);
    }
    tick(g);
    press(g, keys::KEY_ENTER);
}

fn chat_has(g: &Game, needle: &str) -> bool {
    g.chat().log().iter().any(|m| m.text.contains(needle))
}

#[test]
fn two_games_play_together_over_loopback() {
    let da = TempDir::new("host");
    let db = TempDir::new("client");
    let dport = 20_000 + (std::process::id() % 10_000) as u16;
    let mut a = game(&da, dport);
    let mut b = game(&db, dport);

    // L'hôte crée un monde et l'ouvre au réseau.
    a.command(new_world("Lan"));
    assert_eq!(a.state(), GameState::Playing);
    for _ in 0..5 {
        tick(&mut a);
    }
    assert_eq!(a.lan_role(), LanRole::Off);
    a.open_to_lan();
    assert_eq!(a.lan_role(), LanRole::Host);
    tick(&mut a);
    let port = host_port(&a);
    assert!(
        chat_has(&a, "open to LAN"),
        "l'hôte est prévenu dans le chat"
    );

    // Le client voit la partie dans la liste (découverte UDP).
    b.command(UiCommand::SwitchState(GameState::Multiplayer));
    run_until(&mut a, &mut b, "partie découverte", |_, b| {
        !b.lan_view().servers.is_empty()
    });
    let found = b.lan_view().servers[0].clone();
    assert_eq!(found.world, "Lan");
    assert_eq!(found.max_players, 8);
    assert!(found.address.ends_with(&format!(":{port}")), "{found:?}");

    // Une adresse erronée est expliquée sans planter.
    b.command(UiCommand::JoinLan {
        address: "pas une adresse :::".into(),
        name: "Tom".into(),
    });
    tick(&mut b);
    assert!(
        b.lan_view().message.as_ref().is_some_and(|m| m.1),
        "erreur affichée"
    );
    assert_eq!(b.lan_role(), LanRole::Off);

    // Rejoindre.
    b.command(UiCommand::JoinLan {
        address: format!("127.0.0.1:{port}"),
        name: "Tom".into(),
    });
    assert_eq!(b.lan_role(), LanRole::Client);
    run_until(&mut a, &mut b, "client dans le monde", |_, b| {
        b.state() == GameState::Playing && !b.lan_view().loading_terrain
    });
    assert_eq!(b.session().world.seed(), 31337);
    assert!(b.session().world.chunk_manager().is_remote());
    assert_eq!(b.settings.player_name, "Tom");
    run_until(&mut a, &mut b, "joueurs visibles", |a, b| {
        a.lan_view().players.len() == 2
            && b.lan_view().players.len() == 2
            && a.render_view().avatars.len() == 1
            && b.render_view().avatars.len() == 1
    });
    assert_eq!(a.render_view().avatars[0].name, "Tom");
    assert!(b.lan_view().players.iter().any(|p| p.is_host));
    assert!(b
        .lan_view()
        .players
        .iter()
        .any(|p| p.is_self && p.name == "Tom"));
    assert!(chat_has(&a, "Tom joined"), "annonce d'arrivée");

    // Bloc posé par le client → monde de l'hôte.
    let stone = a.regs.blocks.ids().stone;
    let air = a.regs.blocks.ids().air;
    let cam = b.camera().position;
    let (bx, by, bz) = (
        cam.x.floor() as i32 + 3,
        cam.y.floor() as i32 + 6,
        cam.z.floor() as i32,
    );
    b.world().set_block(bx, by, bz, stone);
    run_until(&mut a, &mut b, "bloc du client chez l'hôte", |a, _| {
        a.world().block(bx, by, bz) == stone
    });
    // Bloc cassé par l'hôte → monde du client.
    a.world().set_block(bx, by, bz, air);
    run_until(&mut a, &mut b, "bloc de l'hôte chez le client", |_, b| {
        b.world().block(bx, by, bz) == air
    });
    // Un bloc de l'hôte loin du client reste cohérent des deux côtés.
    a.world().set_block(bx + 1, by, bz, stone);
    run_until(&mut a, &mut b, "second bloc", |_, b| {
        b.world().block(bx + 1, by, bz) == stone
    });

    // Mob créé par l'hôte → répliqué chez le client.
    let cfg = a.regs.mobs.all()[0].clone();
    let host_pos = a.camera().position;
    let mut rng = nb_core::JavaRandom::new(5);
    nb_ecs::spawn::spawn_mob(
        &mut a.session_mut().sim.entities,
        &cfg,
        f64::from(host_pos.x) + 2.0,
        f64::from(host_pos.y),
        f64::from(host_pos.z),
        &mut rng,
    );
    run_until(&mut a, &mut b, "mob répliqué", |_, b| {
        b.render_view().mobs.iter().any(|m| m.mob_id == cfg.id)
    });

    // Chat dans les deux sens ; une commande de client est refusée.
    type_chat(&mut b, "salut a tous");
    run_until(&mut a, &mut b, "chat du client", |a, b| {
        chat_has(a, "<Tom> salut a tous") && chat_has(b, "<Tom> salut a tous")
    });
    type_chat(&mut a, "bienvenue");
    run_until(&mut a, &mut b, "chat de l'hôte", |a, b| {
        chat_has(b, "bienvenue") && chat_has(a, "bienvenue")
    });
    type_chat(&mut b, "/time set night");
    assert!(chat_has(&b, "Only the host"), "commande refusée au client");

    // Le client part : l'hôte le voit.
    b.command(UiCommand::SaveAndMainMenu);
    assert_eq!(b.state(), GameState::MainMenu);
    assert_eq!(b.lan_role(), LanRole::Off);
    run_until(&mut a, &mut b, "départ vu par l'hôte", |a, _| {
        a.lan_view().players.len() == 1
    });
    assert!(chat_has(&a, "Tom left"));
    assert!(a.render_view().avatars.is_empty());

    // Il revient, puis l'hôte ferme la partie : le client reçoit un message clair.
    b.command(UiCommand::SwitchState(GameState::Multiplayer));
    b.command(UiCommand::JoinLan {
        address: format!("127.0.0.1:{port}"),
        name: "Tom".into(),
    });
    run_until(&mut a, &mut b, "retour du client", |_, b| {
        b.state() == GameState::Playing && !b.lan_view().loading_terrain
    });
    a.close_lan();
    assert_eq!(a.lan_role(), LanRole::Off);
    run_until(&mut a, &mut b, "client prévenu", |_, b| {
        b.state() == GameState::Multiplayer
    });
    let (msg, _) = b
        .lan_view()
        .message
        .clone()
        .expect("message de fin de partie");
    assert!(msg.contains("closed"), "{msg}");
    assert_eq!(b.lan_role(), LanRole::Off);

    // L'hôte peut rouvrir sa partie et la sauvegarde reste intacte.
    a.open_to_lan();
    assert_eq!(a.lan_role(), LanRole::Host);
    a.shutdown();
    b.shutdown();
}

#[test]
fn joining_nobody_reports_a_clear_error() {
    let d = TempDir::new("nobody");
    let mut g = game(&d, 20_000 + ((std::process::id() + 7) % 10_000) as u16);
    g.command(UiCommand::SwitchState(GameState::Multiplayer));
    // Port fermé de la machine : refus immédiat.
    g.command(UiCommand::JoinLan {
        address: "127.0.0.1:1".into(),
        name: "  ".into(),
    });
    let start = Instant::now();
    while g.lan_role() != LanRole::Off {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "pas de réponse d'erreur"
        );
        tick(&mut g);
        std::thread::sleep(Duration::from_millis(5));
    }
    tick(&mut g);
    let (msg, err) = g.lan_view().message.clone().expect("message d'erreur");
    assert!(err);
    assert!(
        msg.contains("refused") || msg.contains("No answer"),
        "{msg}"
    );
    assert_eq!(g.state(), GameState::Multiplayer);
    assert_eq!(
        g.settings.player_name, "Player",
        "nom vide → nom par défaut"
    );
    g.shutdown();
}

#[test]
fn only_named_worlds_can_be_opened_to_lan() {
    let d = TempDir::new("menu");
    let mut g = game(&d, 20_000 + ((std::process::id() + 13) % 10_000) as u16);
    g.open_to_lan();
    assert_eq!(
        g.lan_role(),
        LanRole::Off,
        "le monde du menu n'est pas partageable"
    );
    g.shutdown();
}
