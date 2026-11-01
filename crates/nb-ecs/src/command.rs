//! Interpréteur de commandes du chat (`CommandProcessor`, spec §16). Messages exacts du Java ;
//! nombres toujours avec un point décimal (décision de Gabriel).

use nb_world::WorldConfig;

use crate::components::{GameMode, Health, Player, Transform};
use crate::javafmt::{java_float_to_string, java_format_fixed, java_parse_float, java_trim};
use crate::sim::Simulation;

pub const BASE_GRAVITY: f32 = -32.0;
pub const BASE_JUMP_FORCE: f32 = 8.4;

/// Contrôle de l'heure (implémenté par nb-gfx / nb-game).
pub trait TimeControl {
    /// `setTime(t)` : le renderer applique `t % 1200` (+1200 si négatif).
    fn set_time(&mut self, t: f32);
    /// `setDayLengthSeconds(s)` : le renderer applique `max(1, s)`.
    fn set_day_length_seconds(&mut self, s: f32);
}

const HELP: &str = "Commands: /time, /gamemode, /physics, /player, /default, /kill, /setworldspawn";

/// `%.nf` d'un `float` promu en `double`.
fn fixed(v: f32, n: usize) -> String {
    java_format_fixed(f64::from(v), n)
}

/// `CommandProcessor.executeCommand` : `None` = aucune réponse.
pub fn execute_command(
    command: &str,
    sim: &mut Simulation,
    cfg: &mut WorldConfig,
    time: &mut dyn TimeControl,
    allow_cheats: bool,
) -> Option<String> {
    if command.is_empty() {
        return None;
    }
    if command == "/help" {
        return Some(format!(
            "{HELP}  (type /help <command> for details)"
        ));
    }
    if let Some(rest) = command.strip_prefix("/help ") {
        return Some(help(&java_trim(rest).to_lowercase()));
    }
    if !allow_cheats {
        return Some("Cheats are not enabled in this world.".into());
    }
    let reply = if command == "/kill" {
        sim.respawn_player(cfg);
        "Respawned at world spawn.".to_owned()
    } else if command == "/setworldspawn" {
        set_world_spawn(sim, cfg)
    } else if let Some(rest) = command.strip_prefix("/time ") {
        time_group(java_trim(rest), cfg, time)
    } else if let Some(rest) = command.strip_prefix("/gamemode ") {
        game_mode(&java_trim(rest).to_lowercase(), sim)
    } else if let Some(rest) = command.strip_prefix("/physics ") {
        physics_group(java_trim(rest), cfg)
    } else if let Some(rest) = command.strip_prefix("/player ") {
        player_group(java_trim(rest), sim, cfg)
    } else if command == "/default" {
        defaults(sim, cfg)
    } else {
        format!("Unknown command: {command}")
    };
    Some(reply)
}

fn player_component<'a, C: hecs::Component>(sim: &'a mut Simulation) -> Option<&'a mut C> {
    let p = sim.player?;
    sim.entities.get_mut::<C>(p)
}

fn set_world_spawn(sim: &mut Simulation, cfg: &mut WorldConfig) -> String {
    let Some(t) = sim.player.and_then(|p| sim.entities.copy::<Transform>(p)) else {
        return "Player not found.".into();
    };
    cfg.spawn = Some([t.pos.x, t.pos.y, t.pos.z]);
    format!(
        "World spawn point set to ({}, {}, {}).",
        java_format_fixed(t.pos.x, 1),
        java_format_fixed(t.pos.y, 1),
        java_format_fixed(t.pos.z, 1)
    )
}

fn help(args: &str) -> String {
    let s = match args {
        "" => HELP,
        "time" => "/time set <day|noon|sunset|night|midnight|sunrise|number>  |  /time speed <seconds>",
        "time set" => "/time set \u{2014} presets: day, noon, sunset, night, midnight, sunrise  |  or a number (0.0\u{2013}1800.0)",
        "time speed" => "/time speed <seconds> \u{2014} day length in seconds (min: 1.0)",
        "gamemode" => "/gamemode <creative|survival>  (alias: 1 or 0)",
        "physics" => "/physics gravity <mult>  |  /physics falldamage <on|off>",
        "physics gravity" => "/physics gravity <multiplier> \u{2014} 1.0 = default, 2.0 = double, -1.0 = antigravity, 0 = none",
        "physics falldamage" => "/physics falldamage <on|off>",
        "player" => "/player health|damage|walkspeed|sprintspeed|flyspeed|jumpforce <value>",
        "player health" => "/player health <amount> \u{2014} max HP (> 0); hearts scale to always show 10",
        "player damage" => "/player damage <amount> \u{2014} HP dealt per hit (>= 0, default 1.0)",
        "player walkspeed" => "/player walkspeed <speed> \u{2014} walk speed in units/sec (default 4.317)",
        "player sprintspeed" => "/player sprintspeed <speed> \u{2014} sprint speed in units/sec (default 5.612)",
        "player flyspeed" => "/player flyspeed <speed> \u{2014} fly speed in units/sec (default 20.0)",
        "player jumpforce" => "/player jumpforce <multiplier> \u{2014} 1.0 = default, 2.0 = double, must be >= 0",
        "default" => "/default \u{2014} reset all command-tweaked values to their defaults",
        "kill" => "/kill \u{2014} respawn at the world spawn point",
        "setworldspawn" => "/setworldspawn \u{2014} set the world spawn point to your current position",
        other => return format!("No help found for: {other}  |  Try /help for top-level commands"),
    };
    s.to_owned()
}

fn time_group(args: &str, cfg: &mut WorldConfig, time: &mut dyn TimeControl) -> String {
    if let Some(rest) = args.strip_prefix("set ") {
        let value = java_trim(rest).to_lowercase();
        let preset = match value.as_str() {
            "day" => Some(750.0),
            "noon" => Some(900.0),
            "sunset" => Some(0.0),
            "night" => Some(150.0),
            "midnight" => Some(300.0),
            "sunrise" => Some(600.0),
            _ => None,
        };
        if let Some(t) = preset {
            time.set_time(t);
            return format!("Time set to {value}.");
        }
        return match java_parse_float(&value) {
            Some(t) => {
                time.set_time(t);
                format!("Time set to {value}.")
            }
            None => format!("Invalid time value: {value}"),
        };
    }
    if let Some(rest) = args.strip_prefix("speed ") {
        let val = java_trim(rest);
        return match java_parse_float(val) {
            None => format!("Invalid value: {val}"),
            Some(s) if s < 1.0 => "Day length must be at least 1 second.".into(),
            Some(s) => {
                time.set_day_length_seconds(s);
                cfg.day_length_seconds = s;
                format!("Day length set to {}s per cycle.", java_float_to_string(s))
            }
        };
    }
    "Usage: /time set <value|day|night|...> | /time speed <seconds>".into()
}

fn game_mode(value: &str, sim: &mut Simulation) -> String {
    let Some(p) = player_component::<Player>(sim) else {
        return "Player not found.".into();
    };
    match value {
        "creative" | "1" => {
            p.game_mode = GameMode::Creative;
            "Gamemode set to Creative.".into()
        }
        "survival" | "0" => {
            p.game_mode = GameMode::Survival;
            p.flying = false;
            if let Some(h) = player_component::<Health>(sim) {
                h.health = h.max_health;
                h.alive = true;
            }
            "Gamemode set to Survival.".into()
        }
        other => format!("Unknown gamemode: {other}"),
    }
}

fn physics_group(args: &str, cfg: &mut WorldConfig) -> String {
    if let Some(rest) = args.strip_prefix("gravity ") {
        return match java_parse_float(rest) {
            Some(scale) => {
                cfg.gravity = scale * BASE_GRAVITY;
                format!(
                    "Gravity set to {}x ({} units/s^2).",
                    fixed(scale, 2),
                    fixed(cfg.gravity, 1)
                )
            }
            None => "Invalid value. Usage: /physics gravity <multiplier>  (1.0 = default, -1.0 = antigravity)".into(),
        };
    }
    if let Some(rest) = args.strip_prefix("falldamage ") {
        return match java_trim(rest).to_lowercase().as_str() {
            "on" | "true" => {
                cfg.fall_damage_enabled = true;
                "Fall damage enabled.".into()
            }
            "off" | "false" => {
                cfg.fall_damage_enabled = false;
                "Fall damage disabled.".into()
            }
            _ => "Usage: /physics falldamage on|off".into(),
        };
    }
    "Usage: /physics gravity <n> | /physics falldamage on|off".into()
}

/// Champ de vitesse modifié par `/player walkspeed|sprintspeed|flyspeed`.
#[derive(Clone, Copy)]
enum SpeedField {
    Walk,
    Sprint,
    Fly,
}

fn player_group(args: &str, sim: &mut Simulation, cfg: &mut WorldConfig) -> String {
    if let Some(rest) = args.strip_prefix("health ") {
        let Some(amount) = java_parse_float(rest) else {
            return "Invalid value. Usage: /player health <amount>".into();
        };
        if amount <= 0.0 {
            return "Health must be > 0.".into();
        }
        let Some(h) = player_component::<Health>(sim) else {
            return "Player health not found.".into();
        };
        let ratio = if h.max_health > 0.0 {
            h.health / h.max_health
        } else {
            1.0
        };
        h.max_health = amount;
        h.health = crate::jmath::jmax(0.0, ratio * amount);
        return format!("Max health set to {}.", fixed(amount, 1));
    }
    if let Some(rest) = args.strip_prefix("damage ") {
        let Some(amount) = java_parse_float(rest) else {
            return "Invalid value. Usage: /player damage <amount>".into();
        };
        if amount < 0.0 {
            return "Damage must be >= 0.".into();
        }
        cfg.attack_damage = amount;
        return format!("Attack damage set to {}.", fixed(amount, 1));
    }
    let speeds = [
        ("walkspeed ", SpeedField::Walk, "Walk", "walkspeed"),
        ("sprintspeed ", SpeedField::Sprint, "Sprint", "sprintspeed"),
        ("flyspeed ", SpeedField::Fly, "Fly", "flyspeed"),
    ];
    for (prefix, field, label, usage) in speeds {
        if let Some(rest) = args.strip_prefix(prefix) {
            let Some(speed) = java_parse_float(rest) else {
                return format!("Invalid value. Usage: /player {usage} <speed>");
            };
            if speed < 0.0 {
                return "Speed must be >= 0.".into();
            }
            match field {
                SpeedField::Walk => cfg.walk_speed = speed,
                SpeedField::Sprint => cfg.sprint_speed = speed,
                SpeedField::Fly => cfg.fly_speed = speed,
            }
            return format!("{label} speed set to {}.", fixed(speed, 3));
        }
    }
    if let Some(rest) = args.strip_prefix("jumpforce ") {
        let Some(scale) = java_parse_float(rest) else {
            return "Invalid value. Usage: /player jumpforce <multiplier>  (1.0 = default)".into();
        };
        if scale < 0.0 {
            return "Jump force multiplier must be >= 0.".into();
        }
        cfg.jump_force = scale * BASE_JUMP_FORCE;
        return format!(
            "Jump force set to {}x ({} units/s).",
            fixed(scale, 2),
            fixed(cfg.jump_force, 1)
        );
    }
    "Usage: /player health|damage|walkspeed|sprintspeed|flyspeed|jumpforce <value>".into()
}

/// `/default` : valeurs de gameplay par défaut (la durée du jour du renderer n'est **pas**
/// remise à jour, B13).
fn defaults(sim: &mut Simulation, cfg: &mut WorldConfig) -> String {
    cfg.gravity = BASE_GRAVITY;
    cfg.jump_force = BASE_JUMP_FORCE;
    cfg.fall_damage_enabled = true;
    cfg.day_length_seconds = 1200.0;
    cfg.walk_speed = 4.317;
    cfg.sprint_speed = 5.612;
    cfg.fly_speed = 20.0;
    cfg.attack_damage = 1.0;
    if let Some(h) = player_component::<Health>(sim) {
        h.max_health = 10.0;
        h.health = crate::jmath::jmin(h.health, h.max_health);
    }
    "All values reset to defaults.".into()
}
