//! Tests du raccord son ↔ jeu avec un moteur « enregistreur » : on vérifie les demandes faites
//! au moteur audio sans jamais ouvrir de périphérique.

use std::path::PathBuf;
use std::sync::Arc;

use nb_audio::{AudioEngine, Cmd, Material, MobCall, Sfx, Theme};
use nb_core::glam::IVec3;
use nb_ecs::{EcsEvent, GameMode};
use nb_game::audio::{AudioCtx, AudioDirector};
use nb_game::{Game, GameOptions, GameState, NewWorld, Registries, UiCommand, TIME_STEP};
use nb_gfx::SkyClock;
use nb_mesh::BlockTable;
use nb_world::WorldConfig;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("nb-audio-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("settings.json"), r#"{ "renderDistance": 2 }"#).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn game(dir: &TempDir) -> Game {
    let regs = Registries::builtin();
    let table = Arc::new(BlockTable::from_registry(&regs.blocks, |_| 0));
    let mut o = GameOptions::new(&dir.0);
    o.menu_seed = Some(1234);
    o.rng_seed = Some(99);
    o.mesh_threads = Some(1);
    let mut g = Game::new(o, regs, table);
    g.set_audio_engine(AudioEngine::recording());
    g
}

fn sfx_of(cmds: &[Cmd]) -> Vec<Sfx> {
    cmds.iter()
        .filter_map(|c| match c {
            Cmd::Play(r) => Some(r.sfx),
            _ => None,
        })
        .collect()
}

#[test]
fn volumes_are_sent_at_start_and_on_apply() {
    let dir = TempDir::new("vol");
    let mut g = game(&dir);
    let cmds = g.audio().engine().take_recorded();
    let v = cmds
        .iter()
        .find_map(|c| match c {
            Cmd::Volumes(v) => Some(*v),
            _ => None,
        })
        .expect("volumes envoyés");
    assert_eq!((v.master, v.music), (1.0, 0.5));

    let mut s = g.settings.clone();
    s.set_volumes([0.4, 0.0, 0.3, 0.2, 0.1]);
    g.command(UiCommand::ApplySettings(Box::new(s)));
    let cmds = g.audio().engine().take_recorded();
    let v = cmds
        .iter()
        .rev()
        .find_map(|c| match c {
            Cmd::Volumes(v) => Some(*v),
            _ => None,
        })
        .expect("nouveaux volumes");
    assert_eq!(
        (v.master, v.music, v.ambient, v.effects, v.creatures),
        (0.4, 0.0, 0.3, 0.2, 0.1)
    );
    assert_eq!(g.settings.volumes(), [0.4, 0.0, 0.3, 0.2, 0.1]);
}

#[test]
fn ui_commands_click_and_overlays_open_and_close() {
    let dir = TempDir::new("ui");
    let mut g = game(&dir);
    g.audio().engine().take_recorded();
    g.command(UiCommand::SwitchState(GameState::WorldSelect));
    assert_eq!(
        sfx_of(&g.audio().engine().take_recorded()),
        vec![Sfx::UiClick]
    );

    g.command(UiCommand::SwitchState(GameState::MainMenu));
    g.command(UiCommand::CreateWorld(Box::new(NewWorld {
        name: "Son".into(),
        seed_text: "7".into(),
        game_mode: GameMode::Creative,
        allow_cheats: true,
        config: WorldConfig::default(),
    })));
    g.audio().engine().take_recorded();
    g.switch_state(GameState::Paused);
    assert!(sfx_of(&g.audio().engine().take_recorded()).contains(&Sfx::UiOpen));
    g.switch_state(GameState::Playing);
    assert!(sfx_of(&g.audio().engine().take_recorded()).contains(&Sfx::UiClose));
}

#[test]
fn in_world_frames_publish_listener_ambience_and_music() {
    let dir = TempDir::new("world");
    let mut g = game(&dir);
    g.command(UiCommand::CreateWorld(Box::new(NewWorld {
        name: "Monde".into(),
        seed_text: "12345".into(),
        game_mode: GameMode::Creative,
        allow_cheats: true,
        config: WorldConfig::default(),
    })));
    g.audio().engine().take_recorded();
    for _ in 0..30 {
        g.update(TIME_STEP);
    }
    let cmds = g.audio().engine().take_recorded();
    assert!(cmds.iter().any(|c| matches!(c, Cmd::Listener(_))));
    assert!(cmds.iter().any(|c| matches!(c, Cmd::Ambience(_))));
    assert!(cmds.iter().any(|c| matches!(c, Cmd::Environment(_))));
    assert!(cmds
        .iter()
        .any(|c| matches!(c, Cmd::MusicMode(nb_audio::MusicMode::Sparse))));
}

#[test]
fn menu_plays_continuous_menu_music() {
    let dir = TempDir::new("menu");
    let mut g = game(&dir);
    for _ in 0..30 {
        g.update(TIME_STEP);
    }
    let cmds = g.audio().engine().take_recorded();
    assert!(cmds
        .iter()
        .any(|c| matches!(c, Cmd::MusicMode(nb_audio::MusicMode::Continuous))));
    assert_eq!(g.audio().theme(), Theme::Menu);
}

#[test]
fn block_events_play_positioned_material_sounds() {
    let dir = TempDir::new("blocks");
    let mut g = game(&dir);
    g.command(UiCommand::CreateWorld(Box::new(NewWorld {
        name: "Blocs".into(),
        seed_text: "1".into(),
        game_mode: GameMode::Creative,
        allow_cheats: true,
        config: WorldConfig::default(),
    })));
    g.run_ticks(5);
    let ids = *g.regs.blocks.ids();
    let mut d = AudioDirector::new(AudioEngine::recording());
    let sky = SkyClock::new();
    let ctx = AudioCtx {
        session: g.session(),
        regs: &g.regs,
        sky: &sky,
        state: GameState::Playing,
    };
    let pos = IVec3::new(3, 70, -4);
    d.on_tick(
        &ctx,
        &[
            EcsEvent::BlockBroken {
                pos,
                block: ids.stone,
            },
            EcsEvent::BlockPlaced {
                pos,
                block: ids.wood,
            },
            EcsEvent::BlockBroken {
                pos,
                block: ids.air,
            },
        ],
        &[],
        0.05,
    );
    let cmds = d.engine().take_recorded();
    let plays: Vec<_> = cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Play(r) => Some(*r),
            _ => None,
        })
        .collect();
    assert_eq!(plays.len(), 2, "l'air ne fait aucun bruit : {plays:?}");
    assert_eq!(plays[0].sfx, Sfx::Break(Material::Stone));
    assert_eq!(plays[1].sfx, Sfx::Place(Material::Wood));
    assert_eq!(plays[0].pos, Some([3.5, 70.5, -3.5]));
}

#[test]
fn walking_makes_footsteps_and_unknown_mob_events_are_ignored() {
    let dir = TempDir::new("steps");
    let mut g = game(&dir);
    g.command(UiCommand::CreateWorld(Box::new(NewWorld {
        name: "Pas".into(),
        seed_text: "1".into(),
        game_mode: GameMode::Survival,
        allow_cheats: true,
        config: WorldConfig::default(),
    })));
    // Laisse le terrain se charger et le joueur se poser.
    let t0 = std::time::Instant::now();
    while t0.elapsed() < std::time::Duration::from_secs(20) {
        g.update(TIME_STEP);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let grounded = g
            .session()
            .player()
            .and_then(|p| g.session().sim.entities.copy::<nb_ecs::Velocity>(p))
            .is_some_and(|v| v.grounded);
        if grounded && g.ticks() > 60 {
            break;
        }
    }
    let mut d = AudioDirector::new(AudioEngine::recording());
    let sky = SkyClock::new();
    let pid = g.session().player().unwrap();
    // Marche simulée : on déplace le joueur de 0,3 bloc par tick.
    let mut played = Vec::new();
    for i in 0..40 {
        g.session_mut()
            .sim
            .entities
            .get_mut::<nb_ecs::Transform>(pid)
            .unwrap()
            .pos
            .x += 0.3;
        let ctx = AudioCtx {
            session: g.session(),
            regs: &g.regs,
            sky: &sky,
            state: GameState::Playing,
        };
        let ev: &[EcsEvent] = if i == 3 {
            &[EcsEvent::MobHit {
                entity: nb_ecs::EntityId(9999),
            }]
        } else {
            &[]
        };
        d.on_tick(&ctx, ev, &[], 0.05);
        played.extend(sfx_of(&d.engine().take_recorded()));
    }
    let steps = played.iter().filter(|s| matches!(s, Sfx::Step(_))).count();
    assert!(
        (5..=9).contains(&steps),
        "12 blocs à ~1,6 bloc par pas : {steps} ({played:?})"
    );
    assert!(!played
        .iter()
        .any(|s| matches!(s, Sfx::Mob(_, MobCall::Hurt))));
}
