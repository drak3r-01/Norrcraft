//! Tests de comportement : commandes du chat (§16), messages exacts, point décimal.

mod common;

use common::Harness;
use nb_core::glam::DVec3;
use nb_data::BlockId;
use nb_ecs::{execute_command, GameAction, GameMode, ItemStack};

fn with_player(mode: GameMode) -> Harness {
    let mut h = Harness::new();
    h.stone_floor();
    h.spawn_player(DVec3::new(0.5, 65.0, 0.5), mode);
    h
}

fn say(h: &mut Harness, cmd: &str) -> String {
    h.cmd(cmd)
        .unwrap_or_else(|| panic!("pas de réponse à {cmd:?}"))
}

#[test]
fn help_and_dispatch() {
    let mut h = with_player(GameMode::Survival);
    assert_eq!(h.cmd(""), None);
    assert_eq!(
        say(&mut h, "/help"),
        "Commands: /time, /gamemode, /physics, /player, /default, /kill, /setworldspawn  (type /help <command> for details)"
    );
    assert_eq!(
        say(&mut h, "/help "),
        "Commands: /time, /gamemode, /physics, /player, /default, /kill, /setworldspawn"
    );
    assert_eq!(
        say(&mut h, "/help  TIME SET "),
        "/time set \u{2014} presets: day, noon, sunset, night, midnight, sunrise  |  or a number (0.0\u{2013}1800.0)"
    );
    // Les espaces internes ne sont pas normalisés (comme en Java).
    assert_eq!(
        say(&mut h, "/help time  set"),
        "No help found for: time  set  |  Try /help for top-level commands"
    );
    assert_eq!(
        say(&mut h, "/help player jumpforce"),
        "/player jumpforce <multiplier> \u{2014} 1.0 = default, 2.0 = double, must be >= 0"
    );
    assert_eq!(
        say(&mut h, "/help nope"),
        "No help found for: nope  |  Try /help for top-level commands"
    );
    assert_eq!(say(&mut h, "/time"), "Unknown command: /time");
    assert_eq!(say(&mut h, "/Kill"), "Unknown command: /Kill");
    assert_eq!(say(&mut h, "hello"), "Unknown command: hello");
}

#[test]
fn cheats_disabled_blocks_everything_but_help() {
    let mut h = with_player(GameMode::Survival);
    let mut run = |c: &str| execute_command(c, &mut h.sim, &mut h.cfg, &mut h.time, false);
    assert_eq!(
        run("/kill").unwrap(),
        "Cheats are not enabled in this world."
    );
    assert_eq!(
        run("/whatever").unwrap(),
        "Cheats are not enabled in this world."
    );
    assert!(run("/help").unwrap().starts_with("Commands:"));
    assert!(run("/help kill").unwrap().starts_with("/kill"));
}

#[test]
fn time_commands() {
    let mut h = with_player(GameMode::Survival);
    for (name, t) in [
        ("day", 750.0),
        ("noon", 900.0),
        ("sunset", 0.0),
        ("night", 150.0),
        ("midnight", 300.0),
        ("sunrise", 600.0),
    ] {
        assert_eq!(
            say(&mut h, &format!("/time set {name}")),
            format!("Time set to {name}.")
        );
        assert_eq!(h.time.time, Some(t));
    }
    assert_eq!(say(&mut h, "/time set NOON"), "Time set to noon.");
    assert_eq!(say(&mut h, "/time set 123.5"), "Time set to 123.5.");
    assert_eq!(h.time.time, Some(123.5));
    assert_eq!(say(&mut h, "/time set 1E2"), "Time set to 1e2.");
    assert_eq!(say(&mut h, "/time set abc"), "Invalid time value: abc");
    assert_eq!(
        say(&mut h, "/time set"),
        "Usage: /time set <value|day|night|...> | /time speed <seconds>"
    );
    assert_eq!(
        say(&mut h, "/time speed 600"),
        "Day length set to 600.0s per cycle."
    );
    assert_eq!(h.time.day_length, Some(600.0));
    assert_eq!(h.cfg.day_length_seconds, 600.0);
    assert_eq!(
        say(&mut h, "/time speed 0.5"),
        "Day length must be at least 1 second."
    );
    assert_eq!(say(&mut h, "/time speed x"), "Invalid value: x");
    assert_eq!(
        say(&mut h, "/time speed 1.0E-1"),
        "Day length must be at least 1 second."
    );
    assert_eq!(
        say(&mut h, "/time speed 12345678"),
        "Day length set to 1.2345678E7s per cycle."
    );
}

#[test]
fn gamemode_commands() {
    let mut h = with_player(GameMode::Creative);
    h.p_mut().flying = true;
    assert_eq!(say(&mut h, "/gamemode 1"), "Gamemode set to Creative.");
    assert!(h.p().flying, "créatif ne coupe pas le vol");
    let pid = h.player();
    h.sim
        .entities
        .get_mut::<nb_ecs::Health>(pid)
        .unwrap()
        .health = 3.0;
    assert_eq!(
        say(&mut h, "/gamemode SURVIVAL"),
        "Gamemode set to Survival."
    );
    let p = h.p();
    assert_eq!(p.game_mode, GameMode::Survival);
    assert!(!p.flying);
    assert_eq!(h.h(pid).health, 10.0);
    assert_eq!(say(&mut h, "/gamemode 0"), "Gamemode set to Survival.");
    assert_eq!(
        say(&mut h, "/gamemode creative"),
        "Gamemode set to Creative."
    );
    assert_eq!(
        say(&mut h, "/gamemode adventure"),
        "Unknown gamemode: adventure"
    );

    let mut empty = Harness::new();
    assert_eq!(say(&mut empty, "/gamemode 1"), "Player not found.");
    assert_eq!(say(&mut empty, "/setworldspawn"), "Player not found.");
    assert_eq!(
        say(&mut empty, "/player health 5"),
        "Player health not found."
    );
}

#[test]
fn physics_commands_affect_player_and_mobs() {
    let mut h = with_player(GameMode::Survival);
    assert_eq!(
        say(&mut h, "/physics gravity 2"),
        "Gravity set to 2.00x (-64.0 units/s^2)."
    );
    assert_eq!(h.cfg.gravity, -64.0);
    assert_eq!(
        say(&mut h, "/physics gravity -0.5"),
        "Gravity set to -0.50x (16.0 units/s^2)."
    );
    assert_eq!(
        say(&mut h, "/physics gravity NaN"),
        "Gravity set to NaNx (NaN units/s^2)."
    );
    assert!(h.cfg.gravity.is_nan());
    assert_eq!(
        say(&mut h, "/physics gravity x"),
        "Invalid value. Usage: /physics gravity <multiplier>  (1.0 = default, -1.0 = antigravity)"
    );
    assert_eq!(
        say(&mut h, "/physics falldamage OFF"),
        "Fall damage disabled."
    );
    assert!(!h.cfg.fall_damage_enabled);
    assert_eq!(
        say(&mut h, "/physics falldamage true"),
        "Fall damage enabled."
    );
    assert_eq!(
        say(&mut h, "/physics falldamage maybe"),
        "Usage: /physics falldamage on|off"
    );
    assert_eq!(
        say(&mut h, "/physics wind 3"),
        "Usage: /physics gravity <n> | /physics falldamage on|off"
    );
    // Antigravité : le joueur s'envole.
    say(&mut h, "/physics gravity -1");
    let y0 = h.t(h.player()).pos.y;
    h.ticks(30);
    assert!(h.t(h.player()).pos.y > y0 + 1.0);
}

#[test]
fn player_commands() {
    let mut h = with_player(GameMode::Survival);
    let pid = h.player();
    h.sim
        .entities
        .get_mut::<nb_ecs::Health>(pid)
        .unwrap()
        .health = 5.0;
    assert_eq!(say(&mut h, "/player health 20"), "Max health set to 20.0.");
    let hp = h.h(pid);
    assert_eq!((hp.health, hp.max_health), (10.0, 20.0), "ratio conservé");
    assert_eq!(say(&mut h, "/player health 0"), "Health must be > 0.");
    assert_eq!(
        say(&mut h, "/player health lots"),
        "Invalid value. Usage: /player health <amount>"
    );
    assert_eq!(
        say(&mut h, "/player damage 2.5"),
        "Attack damage set to 2.5."
    );
    assert_eq!(h.cfg.attack_damage, 2.5);
    assert_eq!(say(&mut h, "/player damage -1"), "Damage must be >= 0.");
    assert_eq!(
        say(&mut h, "/player walkspeed 4.317"),
        "Walk speed set to 4.317."
    );
    assert_eq!(
        say(&mut h, "/player sprintspeed 7"),
        "Sprint speed set to 7.000."
    );
    assert_eq!(h.cfg.sprint_speed, 7.0);
    assert_eq!(
        say(&mut h, "/player flyspeed 0.0005"),
        "Fly speed set to 0.001."
    );
    assert_eq!(say(&mut h, "/player flyspeed -2"), "Speed must be >= 0.");
    assert_eq!(
        say(&mut h, "/player flyspeed fast"),
        "Invalid value. Usage: /player flyspeed <speed>"
    );
    assert_eq!(
        say(&mut h, "/player jumpforce 2"),
        "Jump force set to 2.00x (16.8 units/s)."
    );
    assert_eq!(h.cfg.jump_force, 16.8);
    assert_eq!(
        say(&mut h, "/player jumpforce -1"),
        "Jump force multiplier must be >= 0."
    );
    assert_eq!(
        say(&mut h, "/player jumpforce x"),
        "Invalid value. Usage: /player jumpforce <multiplier>  (1.0 = default)"
    );
    assert_eq!(
        say(&mut h, "/player fly"),
        "Usage: /player health|damage|walkspeed|sprintspeed|flyspeed|jumpforce <value>"
    );
}

#[test]
fn default_resets_gameplay_values_but_not_renderer_day_length() {
    let mut h = with_player(GameMode::Survival);
    for c in [
        "/physics gravity 3",
        "/player jumpforce 3",
        "/physics falldamage off",
        "/time speed 99",
        "/player walkspeed 1",
        "/player sprintspeed 1",
        "/player flyspeed 1",
        "/player damage 9",
        "/player health 30",
    ] {
        say(&mut h, c);
    }
    h.cfg.mob_spawn_radius = 12;
    assert_eq!(say(&mut h, "/default"), "All values reset to defaults.");
    let c = &h.cfg;
    assert_eq!(
        (
            c.gravity,
            c.jump_force,
            c.fall_damage_enabled,
            c.day_length_seconds
        ),
        (-32.0, 8.4, true, 1200.0)
    );
    assert_eq!(
        (c.walk_speed, c.sprint_speed, c.fly_speed, c.attack_damage),
        (4.317, 5.612, 20.0, 1.0)
    );
    assert_eq!(c.mob_spawn_radius, 12, "rayons de spawn non touchés");
    assert_eq!(
        h.time.day_length,
        Some(99.0),
        "renderer non remis à jour (B13)"
    );
    let hp = h.h(h.player());
    assert_eq!((hp.health, hp.max_health), (10.0, 10.0));
}

#[test]
fn setworldspawn_and_kill() {
    let mut h = with_player(GameMode::Survival);
    let pid = h.player();
    h.sim
        .entities
        .get_mut::<nb_ecs::Transform>(pid)
        .unwrap()
        .pos = DVec3::new(-3.25, 65.0, 7.75);
    assert_eq!(
        say(&mut h, "/setworldspawn"),
        "World spawn point set to (-3.3, 65.0, 7.8)."
    );
    assert_eq!(h.cfg.spawn, Some([-3.25, 65.0, 7.75]));

    let dirt: BlockId = h.ids().dirt;
    h.inv_mut().set_slot(27, Some(ItemStack::new(dirt, 5)));
    h.sim
        .entities
        .get_mut::<nb_ecs::Transform>(pid)
        .unwrap()
        .pos = DVec3::new(10.0, 70.0, 10.0);
    h.p_mut().flying = true;
    h.p_mut().camera.yaw_deg = 45.0;
    assert_eq!(say(&mut h, "/kill"), "Respawned at world spawn.");
    let t = h.t(pid);
    assert_eq!(t.pos, DVec3::new(-3.25, 65.0, 7.75));
    assert!(h.inv().is_empty());
    let p = h.p();
    assert!(p.flying, "vol inchangé");
    assert_eq!(p.camera.yaw_deg, 45.0, "orientation inchangée");
    assert!((p.camera.position.y - 66.62).abs() < 1e-4);

    // Sans spawn enregistré : (0.5, seaLevel + 5, 0.5).
    let mut h = with_player(GameMode::Survival);
    say(&mut h, "/kill");
    assert_eq!(h.t(h.player()).pos, DVec3::new(0.5, 39.0, 0.5));
}

#[test]
fn walk_speed_command_changes_movement() {
    let mut h = with_player(GameMode::Survival);
    h.ticks(30);
    say(&mut h, "/player walkspeed 2");
    h.hold(GameAction::Forward);
    h.ticks(60);
    assert!((h.v(h.player()).v.z + 2.0).abs() < 0.01);
}
