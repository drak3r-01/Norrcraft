//! Codes de touches GLFW (conservés dans `settings.json`) et noms d'affichage (`KeyNames`).
//!
//! Les réglages stockent des codes GLFW ; la conversion winit ⇄ GLFW vit dans `nb-game`.

pub const KEY_SPACE: i32 = 32;
pub const KEY_APOSTROPHE: i32 = 39;
pub const KEY_COMMA: i32 = 44;
pub const KEY_MINUS: i32 = 45;
pub const KEY_PERIOD: i32 = 46;
pub const KEY_SLASH: i32 = 47;
pub const KEY_0: i32 = 48;
pub const KEY_9: i32 = 57;
pub const KEY_SEMICOLON: i32 = 59;
pub const KEY_EQUAL: i32 = 61;
pub const KEY_A: i32 = 65;
pub const KEY_D: i32 = 68;
pub const KEY_E: i32 = 69;
pub const KEY_Q: i32 = 81;
pub const KEY_S: i32 = 83;
pub const KEY_W: i32 = 87;
pub const KEY_Z: i32 = 90;
pub const KEY_LEFT_BRACKET: i32 = 91;
pub const KEY_BACKSLASH: i32 = 92;
pub const KEY_RIGHT_BRACKET: i32 = 93;
pub const KEY_GRAVE_ACCENT: i32 = 96;
pub const KEY_ESCAPE: i32 = 256;
pub const KEY_ENTER: i32 = 257;
pub const KEY_TAB: i32 = 258;
pub const KEY_BACKSPACE: i32 = 259;
pub const KEY_INSERT: i32 = 260;
pub const KEY_DELETE: i32 = 261;
pub const KEY_RIGHT: i32 = 262;
pub const KEY_LEFT: i32 = 263;
pub const KEY_DOWN: i32 = 264;
pub const KEY_UP: i32 = 265;
pub const KEY_PAGE_UP: i32 = 266;
pub const KEY_PAGE_DOWN: i32 = 267;
pub const KEY_HOME: i32 = 268;
pub const KEY_END: i32 = 269;
pub const KEY_CAPS_LOCK: i32 = 280;
pub const KEY_F1: i32 = 290;
pub const KEY_F12: i32 = 301;
pub const KEY_KP_0: i32 = 320;
pub const KEY_KP_9: i32 = 329;
pub const KEY_KP_ENTER: i32 = 335;
pub const KEY_LEFT_SHIFT: i32 = 340;
pub const KEY_LEFT_CONTROL: i32 = 341;
pub const KEY_LEFT_ALT: i32 = 342;
pub const KEY_RIGHT_SHIFT: i32 = 344;
pub const KEY_RIGHT_CONTROL: i32 = 345;
pub const KEY_RIGHT_ALT: i32 = 346;

/// `KeyNames.glfw(keyCode)` : nom affiché pour une touche, `"Key<code>"` si inconnue.
pub fn key_name(code: i32) -> String {
    let fixed = match code {
        KEY_SPACE => "Space",
        KEY_APOSTROPHE => "'",
        KEY_COMMA => ",",
        KEY_MINUS => "-",
        KEY_PERIOD => ".",
        KEY_SLASH => "/",
        KEY_SEMICOLON => ";",
        KEY_EQUAL => "=",
        KEY_LEFT_BRACKET => "[",
        KEY_BACKSLASH => "\\",
        KEY_RIGHT_BRACKET => "]",
        KEY_GRAVE_ACCENT => "`",
        KEY_ESCAPE => "Escape",
        KEY_ENTER => "Enter",
        KEY_TAB => "Tab",
        KEY_BACKSPACE => "Backspace",
        KEY_INSERT => "Insert",
        KEY_DELETE => "Delete",
        KEY_RIGHT => "Right",
        KEY_LEFT => "Left",
        KEY_DOWN => "Down",
        KEY_UP => "Up",
        KEY_PAGE_UP => "PgUp",
        KEY_PAGE_DOWN => "PgDn",
        KEY_HOME => "Home",
        KEY_END => "End",
        KEY_CAPS_LOCK => "CapsLock",
        KEY_LEFT_SHIFT => "L.Shift",
        KEY_LEFT_CONTROL => "L.Ctrl",
        KEY_LEFT_ALT => "L.Alt",
        KEY_RIGHT_SHIFT => "R.Shift",
        KEY_RIGHT_CONTROL => "R.Ctrl",
        KEY_RIGHT_ALT => "R.Alt",
        KEY_KP_ENTER => "NumEnter",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_owned();
    }
    match code {
        KEY_KP_0..=KEY_KP_9 => format!("Num{}", code - KEY_KP_0),
        KEY_F1..=KEY_F12 => format!("F{}", code - KEY_F1 + 1),
        KEY_A..=KEY_Z | KEY_0..=KEY_9 => char::from(code as u8).to_string(),
        _ => format!("Key{code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_java() {
        assert_eq!(key_name(KEY_W), "W");
        assert_eq!(key_name(KEY_SPACE), "Space");
        assert_eq!(key_name(KEY_LEFT_CONTROL), "L.Ctrl");
        assert_eq!(key_name(KEY_F1 + 4), "F5");
        assert_eq!(key_name(KEY_KP_0 + 3), "Num3");
        assert_eq!(key_name(KEY_0), "0");
        assert_eq!(key_name(999), "Key999");
    }
}
