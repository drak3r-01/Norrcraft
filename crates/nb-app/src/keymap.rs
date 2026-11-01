//! Conversion winit → codes GLFW (ceux de `settings.json`). Les deux sont des positions
//! physiques de clavier US, indépendantes de la disposition.

use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// Code GLFW d'une touche physique, `None` si GLFW n'en a pas.
pub fn glfw_key(k: KeyCode) -> Option<i32> {
    use KeyCode::*;
    let letter = |c: u8| Some(i32::from(c));
    Some(match k {
        KeyA => return letter(b'A'),
        KeyB => return letter(b'B'),
        KeyC => return letter(b'C'),
        KeyD => return letter(b'D'),
        KeyE => return letter(b'E'),
        KeyF => return letter(b'F'),
        KeyG => return letter(b'G'),
        KeyH => return letter(b'H'),
        KeyI => return letter(b'I'),
        KeyJ => return letter(b'J'),
        KeyK => return letter(b'K'),
        KeyL => return letter(b'L'),
        KeyM => return letter(b'M'),
        KeyN => return letter(b'N'),
        KeyO => return letter(b'O'),
        KeyP => return letter(b'P'),
        KeyQ => return letter(b'Q'),
        KeyR => return letter(b'R'),
        KeyS => return letter(b'S'),
        KeyT => return letter(b'T'),
        KeyU => return letter(b'U'),
        KeyV => return letter(b'V'),
        KeyW => return letter(b'W'),
        KeyX => return letter(b'X'),
        KeyY => return letter(b'Y'),
        KeyZ => return letter(b'Z'),
        Digit0 => 48,
        Digit1 => 49,
        Digit2 => 50,
        Digit3 => 51,
        Digit4 => 52,
        Digit5 => 53,
        Digit6 => 54,
        Digit7 => 55,
        Digit8 => 56,
        Digit9 => 57,
        Space => 32,
        Quote => 39,
        Comma => 44,
        Minus => 45,
        Period => 46,
        Slash => 47,
        Semicolon => 59,
        Equal => 61,
        BracketLeft => 91,
        Backslash => 92,
        BracketRight => 93,
        Backquote => 96,
        IntlBackslash => 162, // GLFW_KEY_WORLD_2
        Escape => 256,
        Enter => 257,
        Tab => 258,
        Backspace => 259,
        Insert => 260,
        Delete => 261,
        ArrowRight => 262,
        ArrowLeft => 263,
        ArrowDown => 264,
        ArrowUp => 265,
        PageUp => 266,
        PageDown => 267,
        Home => 268,
        End => 269,
        CapsLock => 280,
        ScrollLock => 281,
        NumLock => 282,
        PrintScreen => 283,
        Pause => 284,
        F1 => 290,
        F2 => 291,
        F3 => 292,
        F4 => 293,
        F5 => 294,
        F6 => 295,
        F7 => 296,
        F8 => 297,
        F9 => 298,
        F10 => 299,
        F11 => 300,
        F12 => 301,
        Numpad0 => 320,
        Numpad1 => 321,
        Numpad2 => 322,
        Numpad3 => 323,
        Numpad4 => 324,
        Numpad5 => 325,
        Numpad6 => 326,
        Numpad7 => 327,
        Numpad8 => 328,
        Numpad9 => 329,
        NumpadDecimal => 330,
        NumpadDivide => 331,
        NumpadMultiply => 332,
        NumpadSubtract => 333,
        NumpadAdd => 334,
        NumpadEnter => 335,
        NumpadEqual => 336,
        ShiftLeft => 340,
        ControlLeft => 341,
        AltLeft => 342,
        SuperLeft => 343,
        ShiftRight => 344,
        ControlRight => 345,
        AltRight => 346,
        SuperRight => 347,
        ContextMenu => 348,
        _ => return None,
    })
}

/// Indice de bouton GLFW (0 gauche, 1 droit, 2 milieu).
pub fn glfw_button(b: MouseButton) -> Option<usize> {
    match b {
        MouseButton::Left => Some(0),
        MouseButton::Right => Some(1),
        MouseButton::Middle => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::keys;

    #[test]
    fn matches_nb_data_codes() {
        assert_eq!(glfw_key(KeyCode::KeyW), Some(keys::KEY_W));
        assert_eq!(glfw_key(KeyCode::KeyQ), Some(keys::KEY_Q));
        assert_eq!(glfw_key(KeyCode::Space), Some(keys::KEY_SPACE));
        assert_eq!(glfw_key(KeyCode::Escape), Some(keys::KEY_ESCAPE));
        assert_eq!(glfw_key(KeyCode::Enter), Some(keys::KEY_ENTER));
        assert_eq!(glfw_key(KeyCode::NumpadEnter), Some(keys::KEY_KP_ENTER));
        assert_eq!(glfw_key(KeyCode::ShiftLeft), Some(keys::KEY_LEFT_SHIFT));
        assert_eq!(
            glfw_key(KeyCode::ControlRight),
            Some(keys::KEY_RIGHT_CONTROL)
        );
        assert_eq!(glfw_key(KeyCode::F12), Some(keys::KEY_F12));
        assert_eq!(glfw_key(KeyCode::Numpad9), Some(keys::KEY_KP_9));
        assert_eq!(glfw_key(KeyCode::Slash), Some(keys::KEY_SLASH));
        assert_eq!(glfw_key(KeyCode::Backspace), Some(keys::KEY_BACKSPACE));
        assert_eq!(glfw_key(KeyCode::KeyT), Some(nb_game::input::glfw::KEY_T));
        assert_eq!(
            glfw_key(KeyCode::NumpadDivide),
            Some(nb_game::input::glfw::KEY_KP_DIVIDE)
        );
        assert_eq!(glfw_key(KeyCode::Digit1), Some(nb_game::input::glfw::KEY_1));
        assert_eq!(glfw_key(KeyCode::F13), None);
    }

    #[test]
    fn mouse_buttons() {
        assert_eq!(glfw_button(MouseButton::Left), Some(0));
        assert_eq!(glfw_button(MouseButton::Right), Some(1));
        assert_eq!(glfw_button(MouseButton::Middle), Some(2));
        assert_eq!(glfw_button(MouseButton::Back), None);
    }
}
