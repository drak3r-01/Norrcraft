//! Chat (`ChatController`, `ChatMessage`, `UIManager.renderChatLog/renderChatBox`).
//!
//! Le contrôleur tourne dans tous les états (appelé par la boucle principale). L'exécution des
//! commandes est fournie par l'appelant (`CommandProcessor`, nb-ecs / nb-game).

use crate::draw::{DrawList, Rgba, WHITE, YELLOW};
use crate::input::TypingBuffers;

/// Lignes conservées dans le journal.
pub const MAX_LOG_MESSAGES: usize = 4;
/// Durée d'affichage du journal après fermeture (s).
pub const LOG_DISPLAY_SECONDS: f32 = 5.0;

/// `ChatMessage` : texte + couleur RGB (alpha 1).
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub text: String,
    pub color: Rgba,
}

impl ChatMessage {
    pub fn white(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: WHITE,
        }
    }

    pub fn yellow(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: YELLOW,
        }
    }
}

/// Touches du chat de la frame (actions `OPEN_CHAT`, `SUBMIT_CHAT`, `CANCEL_CHAT` venant
/// d'être pressées ; `OPEN_CHAT` n'existe qu'hors saisie, les deux autres qu'en saisie).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatKeys {
    pub open: bool,
    pub submit: bool,
    pub cancel: bool,
}

/// `ChatController`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chat {
    log: Vec<ChatMessage>,
    active: bool,
    log_timer: f32,
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

    pub fn log_timer(&self) -> f32 {
        self.log_timer
    }

    /// Ajoute une ligne (même règle de taille que les commandes).
    pub fn push(&mut self, msg: ChatMessage) {
        self.log.push(msg);
        self.trim();
    }

    /// `update(dt, inputManager, context)` ; `execute` = `CommandProcessor.executeCommand`
    /// (retour affiché en jaune s'il existe).
    pub fn update(
        &mut self,
        dt: f64,
        keys: ChatKeys,
        buffers: &mut TypingBuffers,
        mut execute: impl FnMut(&str) -> Option<String>,
    ) {
        if !self.active && keys.open {
            self.active = true;
            buffers.typing_mode = true;
            buffers.command.clear();
            buffers.command.push('/');
        }
        if self.active {
            if keys.submit {
                let command = buffers.command.clone();
                self.log.push(ChatMessage::white(command.clone()));
                if let Some(feedback) = execute(&command) {
                    self.log.push(ChatMessage::yellow(feedback));
                }
                self.trim();
                self.close(buffers);
            } else if keys.cancel {
                self.close(buffers);
            }
        } else if self.log_timer > 0.0 {
            self.log_timer -= dt as f32;
        }
    }

    fn close(&mut self, buffers: &mut TypingBuffers) {
        self.active = false;
        buffers.typing_mode = false;
        buffers.command.clear();
        self.log_timer = LOG_DISPLAY_SECONDS;
    }

    fn trim(&mut self) {
        while self.log.len() > MAX_LOG_MESSAGES {
            self.log.remove(0);
        }
    }

    /// `render(uiManager, window, inputManager)`.
    pub fn draw(&self, dl: &mut DrawList, win_w: i32, win_h: i32, buffers: &TypingBuffers) {
        let show_log = self.active || self.log_timer > 0.0;
        if show_log && !self.log.is_empty() {
            draw_log(dl, win_w, win_h, &self.log);
        }
        if self.active {
            draw_box(dl, win_w, win_h, &buffers.command);
        }
    }
}

/// `UIManager.renderChatLog`.
pub fn draw_log(dl: &mut DrawList, win_w: i32, win_h: i32, messages: &[ChatMessage]) {
    let chat_w = win_w as f32 * 0.6;
    let line_h = 22.0;
    let chat_x = 10.0;
    let bottom = win_h as f32 - 40.0 - 10.0;
    let log_h = messages.len() as f32 * line_h;
    let log_y = bottom - log_h - 2.0;
    dl.fill(chat_x, log_y, chat_w, log_h, [0.0, 0.0, 0.0, 0.4]);
    for (i, m) in messages.iter().enumerate() {
        let y = log_y + i as f32 * line_h + 5.0;
        dl.text(&m.text, chat_x + 8.0, y, 2.0, m.color);
    }
}

/// `UIManager.renderChatBox` : `"> " + texte + "_"`.
pub fn draw_box(dl: &mut DrawList, win_w: i32, win_h: i32, text: &str) {
    let chat_w = win_w as f32 * 0.6;
    let chat_h = 40.0;
    let chat_x = 10.0;
    let chat_y = win_h as f32 - chat_h - 10.0;
    dl.fill(chat_x, chat_y, chat_w, chat_h, [0.0, 0.0, 0.0, 0.6]);
    dl.text(
        &format!("> {text}_"),
        chat_x + 10.0,
        chat_y + 12.0,
        2.0,
        WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: ChatKeys = ChatKeys {
        open: true,
        submit: false,
        cancel: false,
    };
    const SUBMIT: ChatKeys = ChatKeys {
        open: false,
        submit: true,
        cancel: false,
    };
    const CANCEL: ChatKeys = ChatKeys {
        open: false,
        submit: false,
        cancel: true,
    };

    #[test]
    fn open_type_submit() {
        let mut chat = Chat::new();
        let mut b = TypingBuffers::default();
        chat.update(0.016, OPEN, &mut b, |_| None);
        assert!(chat.is_active() && b.typing_mode);
        assert_eq!(b.command, "/");
        for c in "/time set 0".chars() {
            b.on_char(c);
        }
        assert_eq!(b.command, "/time set 0");
        let mut seen = String::new();
        chat.update(0.016, SUBMIT, &mut b, |c| {
            seen = c.to_owned();
            Some("Time set".into())
        });
        assert_eq!(seen, "/time set 0");
        assert!(!chat.is_active() && !b.typing_mode && b.command.is_empty());
        assert_eq!(
            chat.log(),
            &[
                ChatMessage::white("/time set 0"),
                ChatMessage::yellow("Time set")
            ]
        );
        assert_eq!(chat.log_timer(), LOG_DISPLAY_SECONDS);
    }

    #[test]
    fn log_keeps_last_four_and_times_out() {
        let mut chat = Chat::new();
        let mut b = TypingBuffers::default();
        for i in 0..3 {
            chat.update(0.0, OPEN, &mut b, |_| None);
            b.command = format!("/c{i}");
            chat.update(0.0, SUBMIT, &mut b, |_| Some(format!("r{i}")));
        }
        let texts: Vec<_> = chat.log().iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["r0", "/c1", "r1", "/c2", "r2"][1..]);
        let mut dl = DrawList::new(1280, 720);
        chat.draw(&mut dl, 1280, 720, &b);
        assert!(!dl.is_empty());
        for _ in 0..6 {
            chat.update(1.0, ChatKeys::default(), &mut b, |_| None);
        }
        assert!(chat.log_timer() <= 0.0);
        let mut dl = DrawList::new(1280, 720);
        chat.draw(&mut dl, 1280, 720, &b);
        assert!(dl.is_empty());
    }

    #[test]
    fn cancel_does_not_log() {
        let mut chat = Chat::new();
        let mut b = TypingBuffers::default();
        chat.update(0.0, OPEN, &mut b, |_| None);
        chat.update(0.0, CANCEL, &mut b, |_| panic!("non exécuté"));
        assert!(chat.log().is_empty() && !chat.is_active());
    }

    #[test]
    fn layout_geometry() {
        let mut dl = DrawList::new(1000, 720);
        draw_box(&mut dl, 1000, 720, "");
        // Fond : (10, 670) 600×40.
        assert_eq!(dl.vertices()[0].pos, [10.0, 670.0]);
        assert_eq!(dl.vertices()[2].pos, [610.0, 710.0]);
        let mut dl = DrawList::new(1000, 720);
        draw_log(
            &mut dl,
            1000,
            720,
            &[ChatMessage::white("a"), ChatMessage::white("b")],
        );
        // logY = 670 - 44 - 2 = 624.
        assert_eq!(dl.vertices()[0].pos, [10.0, 624.0]);
    }
}
