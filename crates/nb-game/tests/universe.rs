//! Univers et palette de blocs par monde (plan Norrcraft, session 0.3) : création, sauvegarde et
//! rechargement d'un monde Norrcraft, retour à vanilla, poignée de main LAN. Aucun GPU.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nb_data::{BlockRegistry, WorldKind, UNASSIGNED_ID};
use nb_game::net::LanRole;
use nb_game::{
    Game, GameMode, GameOptions, GameState, NewWorld, PlatformRequest, Registries, UiCommand,
    TIME_STEP,
};
use nb_mesh::BlockTable;
use nb_net::{ClientEvent, ClientMsg, NetClient, ServerMsg};
use nb_world::save;
use nb_world::WorldConfig;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-uni-{tag}-{}-{nanos}", std::process::id()));
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

/// Fait ce que fait la plateforme quand les registres changent : une table de maillage neuve.
fn serve_requests(g: &mut Game) -> usize {
    let mut rebuilt = 0;
    for r in g.take_requests() {
        if r == PlatformRequest::RebuildBlockTable {
            let table = Arc::new(BlockTable::from_registry(&g.regs.blocks, |_| 0));
            g.set_block_table(table);
            rebuilt += 1;
        }
    }
    rebuilt
}

fn tick(g: &mut Game) {
    g.update(TIME_STEP);
    g.input.end_frame();
    serve_requests(g);
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

fn host_port(a: &Game) -> u16 {
    let addr = a.lan_view().addresses.first().expect("adresse").clone();
    addr.rsplit(':').next().unwrap().parse().unwrap()
}

/// Chunk du joueur chargé ? (génération asynchrone)
fn wait_chunk(g: &mut Game) {
    let start = Instant::now();
    loop {
        let p = g.camera().position;
        if g.world().is_chunk_loaded(p.x.floor() as i32, p.z.floor() as i32) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "chunk non chargé");
        tick(g);
        std::thread::sleep(Duration::from_millis(4));
    }
}

#[test]
fn norrcraft_world_is_created_saved_reloaded_and_vanilla_is_restored() {
    let d = TempDir::new("solo");
    let mut g = game(&d, 21_000 + (std::process::id() % 5_000) as u16);
    assert_eq!(g.regs.kind, WorldKind::Vanilla);

    g.command(new_world("Nord", WorldKind::Norrcraft));
    assert_eq!(g.state(), GameState::Playing);
    assert_eq!(g.regs.kind, WorldKind::Norrcraft);
    assert_ne!(g.regs.blocks.get("granit").id, 0);
    assert_eq!(g.regs.blocks.ids().stone, UNASSIGNED_ID, "pas de pierre vanilla");
    assert!(g.regs.mobs.all().is_empty(), "pas de mob vanilla");
    assert_eq!(g.world().profile().kind, WorldKind::Norrcraft);
    assert!(
        g.take_requests().contains(&PlatformRequest::RebuildBlockTable),
        "la plateforme doit reconstruire la table de blocs"
    );
    g.set_block_table(Arc::new(BlockTable::from_registry(&g.regs.blocks, |_| 0)));

    wait_chunk(&mut g);
    let granit = g.regs.blocks.get("granit").id;
    let p = g.camera().position;
    let (x, y, z) = (p.x.floor() as i32, 60, p.z.floor() as i32);
    g.world().set_block(x, y, z, granit);
    g.save_current();

    // world.json v2 : univers, palette de blocs et biomes.
    let meta = save::load_metadata(&d.0, "Nord").unwrap();
    assert_eq!(meta.format_version, 2);
    assert_eq!(meta.world_kind, WorldKind::Norrcraft);
    assert_eq!(
        meta.block_palette,
        BlockRegistry::for_kind(WorldKind::Norrcraft).palette()
    );
    assert_eq!(meta.biome_palette, ["VIDE"]);

    // Retour au menu : le monde du menu est de nouveau vanilla, avec ses identifiants.
    g.command(UiCommand::SaveAndMainMenu);
    assert_eq!(g.state(), GameState::MainMenu);
    assert_eq!(g.regs.kind, WorldKind::Vanilla);
    assert_eq!(g.regs.blocks.ids().stone, 3);
    assert!(g.take_requests().contains(&PlatformRequest::RebuildBlockTable));
    g.set_block_table(Arc::new(BlockTable::from_registry(&g.regs.blocks, |_| 0)));

    // Rechargement : même univers, le bloc posé est revenu avec la bonne identité.
    g.command(UiCommand::PlayWorld(Box::new(meta)));
    assert_eq!(g.regs.kind, WorldKind::Norrcraft);
    serve_requests(&mut g);
    let start = Instant::now();
    while g.world().block(x, y, z) != granit {
        assert!(start.elapsed() < Duration::from_secs(20), "bloc non rechargé");
        tick(&mut g);
        std::thread::sleep(Duration::from_millis(4));
    }
    g.shutdown();
}

#[test]
fn a_saved_palette_keeps_ids_and_unknown_blocks_become_missing() {
    let d = TempDir::new("palette");
    let mut g = game(&d, 21_000 + ((std::process::id() + 1) % 5_000) as u16);
    g.command(new_world("Vieux", WorldKind::Norrcraft));
    serve_requests(&mut g);
    g.command(UiCommand::SaveAndMainMenu);
    serve_requests(&mut g);

    // Le fichier a été écrit par une autre version : granit à l'identifiant 40, un bloc inconnu
    // à l'identifiant 41.
    let mut meta = save::load_metadata(&d.0, "Vieux").unwrap();
    let mut names = vec![String::new(); 42];
    for (i, n) in [(0, "air"), (4, "water"), (6, "bedrock"), (33, "lava"), (40, "granit")] {
        names[i] = n.to_owned();
    }
    names[41] = "bloc_du_futur".into();
    meta.block_palette = nb_data::BlockPalette::new(names);
    save::save_metadata(&d.0, &meta).unwrap();

    g.command(UiCommand::PlayWorld(Box::new(meta)));
    assert_eq!(g.regs.blocks.get("granit").id, 40);
    assert_eq!(g.regs.blocks.from_id(41).name, "bloc_du_futur");
    // Les blocs de l'univers que le fichier ignorait ont pris des identifiants libres.
    assert_eq!(g.regs.blocks.get("moran").id, 1);
    serve_requests(&mut g);
    g.save_current();
    let again = save::load_metadata(&d.0, "Vieux").unwrap();
    assert_eq!(again.block_palette.names()[40], "granit");
    assert_eq!(again.block_palette.names()[41], "bloc_du_futur", "nom conservé");
    assert_eq!(again.block_palette.names()[1], "moran");
    g.shutdown();
}

#[test]
fn lan_guest_adopts_the_hosts_universe_and_block_ids() {
    let da = TempDir::new("host");
    let db = TempDir::new("guest");
    let dport = 22_000 + (std::process::id() % 5_000) as u16;
    let mut a = game(&da, dport);
    let mut b = game(&db, dport);
    assert_eq!(b.regs.kind, WorldKind::Vanilla);

    a.command(new_world("Nord", WorldKind::Norrcraft));
    for _ in 0..5 {
        tick(&mut a);
    }
    a.open_to_lan();
    assert_eq!(a.lan_role(), LanRole::Host);
    tick(&mut a);
    let port = host_port(&a);

    b.command(UiCommand::SwitchState(GameState::Multiplayer));
    b.command(UiCommand::JoinLan {
        address: format!("127.0.0.1:{port}"),
        name: "Tom".into(),
    });
    run_until(&mut a, &mut b, "invité dans le monde", |_, b| {
        b.state() == GameState::Playing && !b.lan_view().loading_terrain
    });
    assert_eq!(b.regs.kind, WorldKind::Norrcraft);
    assert_eq!(b.world().profile().kind, WorldKind::Norrcraft);
    assert_eq!(b.regs.block_palette(), a.regs.block_palette());
    assert!(b.regs.mobs.all().is_empty());

    // Un bloc posé par l'hôte est le même bloc chez l'invité.
    let granit = a.regs.blocks.get("granit").id;
    let cam = a.camera().position;
    let (x, y, z) = (cam.x.floor() as i32 + 1, cam.y.floor() as i32 + 5, cam.z.floor() as i32);
    a.world().set_block(x, y, z, granit);
    run_until(&mut a, &mut b, "bloc de l'hôte chez l'invité", |_, b| {
        b.world().block(x, y, z) == granit
    });
    assert_eq!(b.regs.blocks.from_id(granit).name, "granit");

    // L'invité part : son menu redevient vanilla.
    b.command(UiCommand::SaveAndMainMenu);
    assert_eq!(b.regs.kind, WorldKind::Vanilla);
    a.shutdown();
    b.shutdown();
}

/// Poignée de main brute : l'hôte Norrcraft répond à un `Hello` donné.
fn hello(port: u16, host: &mut Game, content: Vec<(String, u64)>) -> ServerMsg {
    let mut c = NetClient::connect(format!("127.0.0.1:{port}"));
    let start = Instant::now();
    let mut sent = false;
    loop {
        assert!(start.elapsed() < Duration::from_secs(20), "pas de réponse");
        tick(host);
        for ev in c.poll() {
            match ev {
                ClientEvent::Connected if !sent => {
                    sent = true;
                    c.send(&ClientMsg::Hello {
                        version: nb_net::PROTOCOL_VERSION,
                        name: "Brut".into(),
                        render_distance: 2,
                        content: content.clone(),
                    });
                }
                ClientEvent::Message(m @ (ServerMsg::Welcome(_) | ServerMsg::Refuse(_))) => {
                    c.close();
                    return m;
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

#[test]
fn host_refuses_guests_of_another_universe_or_palette_and_welcomes_the_right_one() {
    let da = TempDir::new("refuse");
    let mut a = game(&da, 23_000 + (std::process::id() % 5_000) as u16);
    a.command(new_world("Nord", WorldKind::Norrcraft));
    for _ in 0..5 {
        tick(&mut a);
    }
    a.open_to_lan();
    tick(&mut a);
    let port = host_port(&a);
    let mine = nb_game::content_fingerprints();
    let norr_fp = mine.iter().find(|(k, _)| k == "norrcraft").unwrap().1;

    // Version du jeu qui ne connaît que vanilla.
    let only_vanilla: Vec<_> = mine.iter().filter(|(k, _)| k == "vanilla").cloned().collect();
    match hello(port, &mut a, only_vanilla) {
        ServerMsg::Refuse(why) => assert!(why.contains("norrcraft") && why.contains("universe"), "{why}"),
        m => panic!("refus attendu, reçu {m:?}"),
    }
    // Même univers, autre palette de blocs.
    match hello(port, &mut a, vec![("norrcraft".into(), norr_fp ^ 1)]) {
        ServerMsg::Refuse(why) => assert!(why.contains("other") && why.contains("blocks"), "{why}"),
        m => panic!("refus attendu, reçu {m:?}"),
    }
    // Contenu identique : accueilli, avec l'univers et la palette du monde.
    match hello(port, &mut a, mine) {
        ServerMsg::Welcome(w) => {
            assert_eq!(w.world_kind, "norrcraft");
            assert_eq!(w.block_palette, a.regs.block_palette().names());
        }
        m => panic!("accueil attendu, reçu {m:?}"),
    }
    a.shutdown();
}
