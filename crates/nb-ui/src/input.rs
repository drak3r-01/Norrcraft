//! Entrées de l'UI : état souris / clavier d'une frame et tampons de saisie
//! (`InputManager.commandBuffer` / `secondaryBuffer`).

/// Entrées d'une frame, telles que les voit l'UI Java.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiInput {
    /// Taille de la fenêtre (`window.getWidth()/getHeight()`).
    pub width: i32,
    pub height: i32,
    pub mouse_x: f64,
    pub mouse_y: f64,
    /// Bouton gauche enfoncé (`glfwGetMouseButton`), lu directement par certains écrans.
    pub mouse_down: bool,
    /// Action `PRIMARY` venant d'être pressée (`isActionJustPressed`, faux en mode saisie).
    pub primary_pressed: bool,
    /// Action `SECONDARY` (clic droit) venant d'être pressée.
    pub secondary_pressed: bool,
    /// Maj gauche ou droite enfoncée.
    pub shift_down: bool,
    /// Défilement vertical de la frame (`window.getScrollY()`).
    pub scroll_y: f64,
    /// Touche Échap enfoncée (`glfwGetKey(ESCAPE)`).
    pub esc_down: bool,
    /// Action `PAUSE` venant d'être pressée.
    pub pause_pressed: bool,
    /// Action `INVENTORY` venant d'être pressée.
    pub inventory_pressed: bool,
    /// Entrée venant d'être pressée pendant une saisie (valider un champ).
    pub enter_pressed: bool,
    /// Échap venu d'être pressé pendant une saisie (annuler).
    pub cancel_pressed: bool,
    pub dt: f64,
}

/// Tampons de saisie de l'`InputManager` (chat, noms de mondes / mobs, germe).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypingBuffers {
    pub typing_mode: bool,
    pub suppress_chat_filter: bool,
    pub use_secondary: bool,
    pub command: String,
    pub secondary: String,
}

impl TypingBuffers {
    /// `glfwSetCharCallback` : ajoute un caractère en mode saisie. Sans
    /// `suppressChatFilter`, un `t`/`T` initial (la touche d'ouverture du chat) est ignoré,
    /// ainsi qu'un second `/` derrière le `/` initial.
    pub fn on_char(&mut self, c: char) {
        if !self.typing_mode {
            return;
        }
        if self.use_secondary {
            self.secondary.push(c);
            return;
        }
        if !self.suppress_chat_filter && self.command.is_empty() && (c == 't' || c == 'T') {
            return;
        }
        if self.command == "/" && c == '/' {
            return;
        }
        self.command.push(c);
    }

    /// Retour arrière (pression ou répétition) en mode saisie.
    pub fn on_backspace(&mut self) {
        if !self.typing_mode {
            return;
        }
        if self.use_secondary {
            self.secondary.pop();
        } else {
            self.command.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_filter() {
        let mut b = TypingBuffers {
            typing_mode: true,
            ..Default::default()
        };
        b.on_char('t');
        assert_eq!(b.command, "");
        b.command.push('/');
        b.on_char('/');
        b.on_char('t');
        b.on_char('p');
        assert_eq!(b.command, "/tp");
        b.on_backspace();
        assert_eq!(b.command, "/t");
    }

    #[test]
    fn suppressed_filter_and_secondary() {
        let mut b = TypingBuffers {
            typing_mode: true,
            suppress_chat_filter: true,
            ..Default::default()
        };
        b.on_char('T');
        assert_eq!(b.command, "T");
        b.use_secondary = true;
        b.on_char('4');
        b.on_char('2');
        b.on_backspace();
        assert_eq!((b.command.as_str(), b.secondary.as_str()), ("T", "4"));
        b.typing_mode = false;
        b.on_char('x');
        b.on_backspace();
        assert_eq!(b.secondary, "4");
    }
}
