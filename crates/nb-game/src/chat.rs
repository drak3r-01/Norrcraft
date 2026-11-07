//! Logique du chat (`ChatController.java`). Le rendu (journal, zone de saisie) relève de nb-ui,
//! qui lit [`Chat::view`].

use crate::input::{Action, InputManager};

/// Lignes de journal conservées.
pub const MAX_LOG_MESSAGES: usize = 4;
/// Durée d'affichage du journal après fermeture.
pub const LOG_DISPLAY_SECONDS: f32 = 5.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatColor {
    White,
    Yellow,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChatMessage {
    pub text: String,
    pub color: ChatColor,
}

/// Ce que nb-ui doit dessiner.
#[derive(Clone, Copy, Debug)]
pub struct ChatView<'a> {
    /// Journal à afficher (vide = rien).
    pub log: &'a [ChatMessage],
    /// Texte de la zone de saisie (`None` = chat fermé).
    pub input: Option<&'a str>,
}

/// Lignes de journal conservées en partie réseau (le chat des autres joueurs défile plus vite).
pub const MAX_LOG_MESSAGES_NET: usize = 7;

#[derive(Clone, Debug, Default)]
pub struct Chat {
    log: Vec<ChatMessage>,
    active: bool,
    log_timer: f32,
    /// Partie LAN : le texte sans « / » est un message pour les autres joueurs.
    net_mode: bool,
}

impl Chat {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn log(&self) -> &[ChatMessage] {
        &self.log
    }

    /// Partie LAN : la saisie commence vide (texte = message, « / » = commande) et le journal
    /// garde plus de lignes.
    pub fn set_net_mode(&mut self, on: bool) {
        self.net_mode = on;
    }

    fn max_lines(&self) -> usize {
        if self.net_mode {
            MAX_LOG_MESSAGES_NET
        } else {
            MAX_LOG_MESSAGES
        }
    }

    /// Ajoute une ligne au journal (message d'un autre joueur, information système) et le
    /// rend visible quelques secondes.
    pub fn push(&mut self, text: impl Into<String>, color: ChatColor) {
        self.log.push(ChatMessage {
            text: text.into(),
            color,
        });
        let excess = self.log.len().saturating_sub(self.max_lines());
        self.log.drain(..excess);
        if !self.active {
            self.log_timer = LOG_DISPLAY_SECONDS;
        }
    }

    pub fn log_timer(&self) -> f32 {
        self.log_timer
    }

    /// `ChatController.update` ; `execute` lance la commande (`CommandProcessor`).
    pub fn update(
        &mut self,
        dt: f64,
        input: &mut InputManager,
        execute: impl FnOnce(&str) -> Option<String>,
    ) {
        if !self.active && input.is_just_pressed(Action::OpenChat) {
            self.active = true;
            input.set_typing_mode(true);
            let buf = input.command_buffer_mut();
            buf.clear();
            if !self.net_mode {
                buf.push('/');
            }
        }
        if self.active {
            if input.is_just_pressed(Action::SubmitChat) {
                let command = input.command_buffer().to_owned();
                // Un message de chat revient par le réseau (« <nom> texte ») : pas d'écho brut.
                if !(self.net_mode && !command.starts_with('/')) {
                    self.log.push(ChatMessage {
                        text: command.clone(),
                        color: ChatColor::White,
                    });
                }
                if let Some(feedback) = execute(&command) {
                    self.log.push(ChatMessage {
                        text: feedback,
                        color: ChatColor::Yellow,
                    });
                }
                let excess = self.log.len().saturating_sub(self.max_lines());
                self.log.drain(..excess);
                self.close(input);
            } else if input.is_just_pressed(Action::CancelChat) {
                self.close(input);
            }
        } else if self.log_timer > 0.0 {
            self.log_timer -= dt as f32;
        }
    }

    fn close(&mut self, input: &mut InputManager) {
        self.active = false;
        input.set_typing_mode(false);
        input.command_buffer_mut().clear();
        self.log_timer = LOG_DISPLAY_SECONDS;
    }

    /// `ChatController.render` (données seulement).
    pub fn view<'a>(&'a self, input: &'a InputManager) -> ChatView<'a> {
        let show_log = self.active || self.log_timer > 0.0;
        ChatView {
            log: if show_log { &self.log } else { &[] },
            input: self.active.then(|| input.command_buffer()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::glfw;
    use nb_data::keys;

    fn press(i: &mut InputManager, code: i32) {
        i.key_event(code, true, false);
        i.update();
        i.key_event(code, false, false);
    }

    #[test]
    fn open_type_submit() {
        let mut i = InputManager::new();
        let mut c = Chat::new();
        press(&mut i, glfw::KEY_T);
        c.update(1.0 / 60.0, &mut i, |_| unreachable!());
        assert!(c.is_active() && i.is_typing_mode());
        assert_eq!(i.command_buffer(), "/");
        i.char_event('t'); // le « t » de l'ouverture n'est pas filtré (tampon non vide)
        for ch in "ime".chars() {
            i.char_event(ch);
        }
        i.update();
        c.update(1.0 / 60.0, &mut i, |_| unreachable!());
        press(&mut i, keys::KEY_ENTER);
        let mut seen = String::new();
        c.update(1.0 / 60.0, &mut i, |cmd| {
            seen = cmd.to_owned();
            Some("ok".into())
        });
        assert_eq!(seen, "/time");
        assert!(!c.is_active() && !i.is_typing_mode());
        assert_eq!(i.command_buffer(), "");
        assert_eq!(c.log().len(), 2);
        assert_eq!(c.log()[1].color, ChatColor::Yellow);
        assert_eq!(c.log_timer(), LOG_DISPLAY_SECONDS);
        assert_eq!(c.view(&i).log.len(), 2);
    }

    #[test]
    fn cancel_and_log_trim_and_timer() {
        let mut i = InputManager::new();
        let mut c = Chat::new();
        for n in 0..3 {
            press(&mut i, keys::KEY_SLASH);
            c.update(0.0, &mut i, |_| None);
            press(&mut i, keys::KEY_KP_ENTER);
            c.update(0.0, &mut i, |_| Some(format!("r{n}")));
        }
        assert_eq!(c.log().len(), MAX_LOG_MESSAGES);
        assert_eq!(c.log()[3].text, "r2");
        press(&mut i, glfw::KEY_T);
        c.update(0.0, &mut i, |_| None);
        press(&mut i, keys::KEY_ESCAPE);
        c.update(0.0, &mut i, |_| unreachable!());
        assert!(!c.is_active());
        assert_eq!(c.log().len(), MAX_LOG_MESSAGES);
        i.update();
        c.update(6.0, &mut i, |_| None);
        assert!(c.view(&i).log.is_empty());
        assert!(c.view(&i).input.is_none());
    }

    #[test]
    fn net_mode_starts_empty_and_does_not_echo_messages() {
        let mut i = InputManager::new();
        let mut c = Chat::new();
        c.set_net_mode(true);
        press(&mut i, glfw::KEY_T);
        c.update(1.0 / 60.0, &mut i, |_| unreachable!());
        assert!(c.is_active());
        assert_eq!(
            i.command_buffer(),
            "",
            "saisie vide : le texte est un message"
        );
        for ch in "salut".chars() {
            i.char_event(ch);
        }
        i.update();
        c.update(1.0 / 60.0, &mut i, |_| unreachable!());
        press(&mut i, keys::KEY_ENTER);
        let mut seen = String::new();
        c.update(1.0 / 60.0, &mut i, |cmd| {
            seen = cmd.to_owned();
            None
        });
        assert_eq!(seen, "salut");
        assert!(c.log().is_empty(), "le message reviendra du réseau");
        // Une commande est répétée dans le journal comme d'habitude.
        press(&mut i, glfw::KEY_T);
        c.update(1.0 / 60.0, &mut i, |_| unreachable!());
        for ch in "/help".chars() {
            i.char_event(ch);
        }
        i.update();
        c.update(1.0 / 60.0, &mut i, |_| unreachable!());
        press(&mut i, keys::KEY_ENTER);
        c.update(1.0 / 60.0, &mut i, |_| Some("aide".into()));
        assert_eq!(c.log().len(), 2);
    }

    #[test]
    fn pushed_lines_show_up_and_the_log_is_bounded() {
        let mut c = Chat::new();
        for n in 0..10 {
            c.push(format!("ligne {n}"), ChatColor::White);
        }
        assert_eq!(c.log().len(), MAX_LOG_MESSAGES);
        assert_eq!(c.log().last().unwrap().text, "ligne 9");
        assert!(c.log_timer() > 0.0, "visible quelques secondes");
        c.set_net_mode(true);
        for n in 0..10 {
            c.push(format!("m{n}"), ChatColor::Yellow);
        }
        assert_eq!(c.log().len(), MAX_LOG_MESSAGES_NET);
        assert_eq!(c.log()[0].text, "m3");
    }
}
