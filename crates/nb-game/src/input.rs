//! Entrées (`InputManager.java`) : la plateforme pousse les événements bruts (codes GLFW), le
//! jeu appelle [`InputManager::update`] une fois par tick.

use std::collections::{BTreeMap, HashSet};

use nb_core::glam::Vec2;
use nb_data::keys;
use nb_ecs::{ActionSet, GameAction, PlayerInput};

/// Codes GLFW absents de `nb_data::keys`.
pub mod glfw {
    pub const KEY_1: i32 = 49;
    pub const KEY_T: i32 = 84;
    pub const KEY_KP_DIVIDE: i32 = 331;
    pub const MOUSE_BUTTON_LEFT: usize = 0;
    pub const MOUSE_BUTTON_RIGHT: usize = 1;
    pub const MOUSE_BUTTON_MIDDLE: usize = 2;
}

/// `InputManager.Action`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Action {
    Forward,
    Backward,
    Left,
    Right,
    Jump,
    Sprint,
    Crouch,
    Pause,
    Inventory,
    Primary,
    Secondary,
    OpenChat,
    SubmitChat,
    CancelChat,
    Drop,
}

impl Action {
    /// Ordre de `Action.values()`.
    pub const ALL: [Action; 15] = [
        Action::Forward,
        Action::Backward,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Sprint,
        Action::Crouch,
        Action::Pause,
        Action::Inventory,
        Action::Primary,
        Action::Secondary,
        Action::OpenChat,
        Action::SubmitChat,
        Action::CancelChat,
        Action::Drop,
    ];

    fn bit(self) -> u16 {
        1 << self as u8
    }

    /// Action de jeu correspondante pour `PlayerInputSystem`.
    pub fn game_action(self) -> Option<GameAction> {
        Some(match self {
            Action::Forward => GameAction::Forward,
            Action::Backward => GameAction::Backward,
            Action::Left => GameAction::Left,
            Action::Right => GameAction::Right,
            Action::Jump => GameAction::Jump,
            Action::Sprint => GameAction::Sprint,
            Action::Crouch => GameAction::Crouch,
            Action::Primary => GameAction::Primary,
            Action::Secondary => GameAction::Secondary,
            Action::Drop => GameAction::Drop,
            _ => return None,
        })
    }

    /// Depuis l'action configurable de `settings.json`.
    pub fn from_settings(a: nb_data::Action) -> Self {
        match a {
            nb_data::Action::Forward => Action::Forward,
            nb_data::Action::Backward => Action::Backward,
            nb_data::Action::Left => Action::Left,
            nb_data::Action::Right => Action::Right,
            nb_data::Action::Jump => Action::Jump,
            nb_data::Action::Sprint => Action::Sprint,
            nb_data::Action::Crouch => Action::Crouch,
            nb_data::Action::Pause => Action::Pause,
            nb_data::Action::Inventory => Action::Inventory,
            nb_data::Action::Drop => Action::Drop,
        }
    }
}

/// Ensemble d'actions.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
struct Actions(u16);

impl Actions {
    fn get(self, a: Action) -> bool {
        self.0 & a.bit() != 0
    }
    fn set(&mut self, a: Action, v: bool) {
        if v {
            self.0 |= a.bit();
        } else {
            self.0 &= !a.bit();
        }
    }
}

/// État des entrées : événements bruts de la plateforme + états d'actions par tick.
#[derive(Debug, Clone)]
pub struct InputManager {
    key_bindings: BTreeMap<Action, i32>,
    mouse_bindings: BTreeMap<Action, usize>,
    current: Actions,
    previous: Actions,

    // Bruts (fenêtre).
    keys_down: HashSet<i32>,
    mouse_down: [bool; 3],
    cursor: (f64, f64),
    scroll_y: f64,

    selected_slot: u8,
    last_mouse: (f64, f64),
    first_mouse: bool,
    mouse_delta: Vec2,

    typing_mode: bool,
    suppress_chat_filter: bool,
    command_buffer: String,
    secondary_buffer: String,
    use_secondary_buffer: bool,
    rebind_waiting: bool,
    rebind_result: Option<i32>,
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InputManager {
    /// Liaisons par défaut du constructeur Java.
    pub fn new() -> Self {
        let key_bindings = [
            (Action::Forward, keys::KEY_W),
            (Action::Backward, keys::KEY_S),
            (Action::Left, keys::KEY_A),
            (Action::Right, keys::KEY_D),
            (Action::Jump, keys::KEY_SPACE),
            (Action::Sprint, keys::KEY_LEFT_SHIFT),
            (Action::Crouch, keys::KEY_LEFT_CONTROL),
            (Action::Pause, keys::KEY_ESCAPE),
            (Action::Inventory, keys::KEY_E),
            (Action::Drop, keys::KEY_Q),
        ]
        .into_iter()
        .collect();
        let mouse_bindings = [
            (Action::Primary, glfw::MOUSE_BUTTON_LEFT),
            (Action::Secondary, glfw::MOUSE_BUTTON_RIGHT),
        ]
        .into_iter()
        .collect();
        Self {
            key_bindings,
            mouse_bindings,
            current: Actions::default(),
            previous: Actions::default(),
            keys_down: HashSet::new(),
            mouse_down: [false; 3],
            cursor: (0.0, 0.0),
            scroll_y: 0.0,
            selected_slot: 0,
            last_mouse: (0.0, 0.0),
            first_mouse: true,
            mouse_delta: Vec2::ZERO,
            typing_mode: false,
            suppress_chat_filter: false,
            command_buffer: String::new(),
            secondary_buffer: String::new(),
            use_secondary_buffer: false,
            rebind_waiting: false,
            rebind_result: None,
        }
    }

    // ── Événements de la plateforme ─────────────────────────────────────────────────────

    /// Touche (code GLFW). `repeat` = répétition automatique d'une touche maintenue.
    pub fn key_event(&mut self, code: i32, pressed: bool, repeat: bool) {
        if pressed && !repeat && self.rebind_waiting {
            // L'écouteur reçoit la touche et rien d'autre ne se passe (callback Java).
            self.rebind_waiting = false;
            self.rebind_result = Some(code);
            self.keys_down.insert(code);
            return;
        }
        if pressed && self.typing_mode && code == keys::KEY_BACKSPACE {
            self.active_buffer_mut().pop();
        }
        if pressed {
            self.keys_down.insert(code);
        } else {
            self.keys_down.remove(&code);
        }
    }

    /// Caractère saisi (`glfwSetCharCallback`).
    pub fn char_event(&mut self, c: char) {
        if !self.typing_mode {
            return;
        }
        if self.use_secondary_buffer {
            self.secondary_buffer.push(c);
            return;
        }
        if !self.suppress_chat_filter && self.command_buffer.is_empty() && (c == 't' || c == 'T') {
            return;
        }
        if self.command_buffer == "/" && c == '/' {
            return;
        }
        self.command_buffer.push(c);
    }

    /// Bouton de souris (0 = gauche, 1 = droit, 2 = milieu).
    pub fn mouse_button(&mut self, button: usize, pressed: bool) {
        if let Some(b) = self.mouse_down.get_mut(button) {
            *b = pressed;
        }
    }

    /// Position absolue du curseur (pixels ; virtuelle et non bornée quand il est capturé).
    pub fn cursor_moved(&mut self, x: f64, y: f64) {
        self.cursor = (x, y);
    }

    /// Molette : dernière valeur reçue (`Window.scrollY`).
    pub fn scroll(&mut self, y: f64) {
        self.scroll_y = y;
    }

    /// Fin de frame (`Window.update`) : la molette retombe à 0.
    pub fn end_frame(&mut self) {
        self.scroll_y = 0.0;
    }

    /// Perte de focus : toutes les touches et boutons sont relâchés.
    pub fn release_all(&mut self) {
        self.keys_down.clear();
        self.mouse_down = [false; 3];
    }

    // ── Tick ────────────────────────────────────────────────────────────────────────────

    fn key(&self, code: i32) -> bool {
        self.keys_down.contains(&code)
    }

    /// `InputManager.update()`.
    pub fn update(&mut self) {
        self.previous = self.current;
        let mut cur = Actions::default();
        for a in Action::ALL {
            let pressed = match a {
                Action::OpenChat => {
                    !self.typing_mode
                        && (self.key(glfw::KEY_T)
                            || self.key(keys::KEY_SLASH)
                            || self.key(glfw::KEY_KP_DIVIDE))
                }
                Action::SubmitChat => {
                    self.typing_mode && (self.key(keys::KEY_ENTER) || self.key(keys::KEY_KP_ENTER))
                }
                Action::CancelChat => self.typing_mode && self.key(keys::KEY_ESCAPE),
                _ if self.typing_mode => false,
                _ => {
                    if let Some(&k) = self.key_bindings.get(&a) {
                        self.key(k)
                    } else if let Some(&b) = self.mouse_bindings.get(&a) {
                        self.mouse_down[b]
                    } else {
                        false
                    }
                }
            };
            cur.set(a, pressed);
        }
        if !self.typing_mode && self.key(keys::KEY_ESCAPE) {
            cur.set(Action::Pause, true);
        }
        self.current = cur;

        if !self.typing_mode {
            for i in 0..9u8 {
                if self.key(glfw::KEY_1 + i32::from(i)) {
                    self.selected_slot = i;
                }
            }
            if self.scroll_y != 0.0 {
                self.selected_slot = nb_ecs::input::scroll_slot(self.selected_slot, self.scroll_y);
            }
        }

        if self.first_mouse || self.typing_mode {
            self.last_mouse = self.cursor;
            self.first_mouse = false;
            self.mouse_delta = Vec2::ZERO;
        } else {
            self.mouse_delta = Vec2::new(
                (self.cursor.0 - self.last_mouse.0) as f32,
                (self.cursor.1 - self.last_mouse.1) as f32,
            );
            self.last_mouse = self.cursor;
        }
    }

    // ── Requêtes ────────────────────────────────────────────────────────────────────────

    pub fn is_pressed(&self, a: Action) -> bool {
        self.current.get(a)
    }

    pub fn is_just_pressed(&self, a: Action) -> bool {
        self.current.get(a) && !self.previous.get(a)
    }

    /// Touche brute enfoncée (écrans qui lisent `glfwGetKey` eux-mêmes).
    pub fn is_key_down(&self, code: i32) -> bool {
        self.key(code)
    }

    /// Bouton brut enfoncé (`glfwGetMouseButton`).
    pub fn is_mouse_down(&self, button: usize) -> bool {
        self.mouse_down.get(button).copied().unwrap_or(false)
    }

    pub fn cursor(&self) -> (f64, f64) {
        self.cursor
    }

    pub fn scroll_y(&self) -> f64 {
        self.scroll_y
    }

    pub fn selected_slot(&self) -> u8 {
        self.selected_slot
    }

    pub fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }

    /// `resetMouse()`.
    pub fn reset_mouse(&mut self) {
        self.first_mouse = true;
    }

    pub fn set_typing_mode(&mut self, v: bool) {
        self.typing_mode = v;
    }

    pub fn is_typing_mode(&self) -> bool {
        self.typing_mode
    }

    pub fn set_suppress_chat_filter(&mut self, v: bool) {
        self.suppress_chat_filter = v;
    }

    pub fn is_chat_filter_suppressed(&self) -> bool {
        self.suppress_chat_filter
    }

    pub fn is_using_secondary_buffer(&self) -> bool {
        self.use_secondary_buffer
    }

    pub fn command_buffer(&self) -> &str {
        &self.command_buffer
    }

    pub fn command_buffer_mut(&mut self) -> &mut String {
        &mut self.command_buffer
    }

    pub fn secondary_buffer(&self) -> &str {
        &self.secondary_buffer
    }

    pub fn secondary_buffer_mut(&mut self) -> &mut String {
        &mut self.secondary_buffer
    }

    pub fn set_use_secondary_buffer(&mut self, v: bool) {
        self.use_secondary_buffer = v;
    }

    fn active_buffer_mut(&mut self) -> &mut String {
        if self.use_secondary_buffer {
            &mut self.secondary_buffer
        } else {
            &mut self.command_buffer
        }
    }

    /// `setRebindListener(listener)` : le prochain appui de touche sera capturé.
    pub fn start_rebind(&mut self) {
        self.rebind_waiting = true;
        self.rebind_result = None;
    }

    /// `setRebindListener(null)`.
    pub fn cancel_rebind(&mut self) {
        self.rebind_waiting = false;
        self.rebind_result = None;
    }

    pub fn is_rebinding(&self) -> bool {
        self.rebind_waiting
    }

    /// Touche capturée par la réaffectation (une seule fois).
    pub fn take_rebind(&mut self) -> Option<i32> {
        self.rebind_result.take()
    }

    /// `applyKeyBindings(settings.keyBindings)` (`putAll`).
    pub fn apply_key_bindings(&mut self, bindings: &BTreeMap<nb_data::Action, i32>) {
        for (&a, &code) in bindings {
            self.key_bindings.insert(Action::from_settings(a), code);
        }
    }

    pub fn key_binding(&self, a: Action) -> Option<i32> {
        self.key_bindings.get(&a).copied()
    }

    /// Instantané pour `PlayerInputSystem` (les actions valent `false` en saisie).
    pub fn player_input(
        &self,
        mouse_locked: bool,
        sensitivity: Option<f32>,
        fov_deg: Option<f32>,
    ) -> PlayerInput {
        let mut pressed = ActionSet::EMPTY;
        let mut just = ActionSet::EMPTY;
        for a in Action::ALL {
            if let Some(g) = a.game_action() {
                if self.is_pressed(a) {
                    pressed.insert(g);
                }
                if self.is_just_pressed(a) {
                    just.insert(g);
                }
            }
        }
        PlayerInput {
            pressed,
            just_pressed: just,
            mouse_delta: self.mouse_delta,
            selected_slot: self.selected_slot,
            mouse_locked,
            sensitivity,
            fov_deg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(i: &mut InputManager) {
        i.update();
    }

    #[test]
    fn just_pressed_is_one_tick() {
        let mut i = InputManager::new();
        i.key_event(keys::KEY_E, true, false);
        tick(&mut i);
        assert!(i.is_just_pressed(Action::Inventory));
        tick(&mut i);
        assert!(i.is_pressed(Action::Inventory));
        assert!(!i.is_just_pressed(Action::Inventory));
        i.key_event(keys::KEY_E, false, false);
        tick(&mut i);
        assert!(!i.is_pressed(Action::Inventory));
    }

    #[test]
    fn escape_forces_pause_even_if_rebound() {
        let mut i = InputManager::new();
        let mut b = BTreeMap::new();
        b.insert(nb_data::Action::Pause, keys::KEY_Q);
        i.apply_key_bindings(&b);
        i.key_event(keys::KEY_ESCAPE, true, false);
        tick(&mut i);
        assert!(i.is_just_pressed(Action::Pause));
    }

    #[test]
    fn typing_mode_blocks_game_actions_and_mouse() {
        let mut i = InputManager::new();
        tick(&mut i);
        i.set_typing_mode(true);
        i.key_event(keys::KEY_W, true, false);
        i.key_event(keys::KEY_ESCAPE, true, false);
        i.cursor_moved(50.0, 20.0);
        tick(&mut i);
        assert!(!i.is_pressed(Action::Forward));
        assert!(!i.is_pressed(Action::Pause));
        assert!(i.is_just_pressed(Action::CancelChat));
        assert_eq!(i.mouse_delta(), Vec2::ZERO);
        let p = i.player_input(true, None, None);
        assert_eq!(p.pressed, ActionSet::EMPTY);
    }

    #[test]
    fn chat_char_filters() {
        let mut i = InputManager::new();
        i.char_event('x');
        assert_eq!(i.command_buffer(), "");
        i.set_typing_mode(true);
        i.char_event('t');
        assert_eq!(i.command_buffer(), "");
        i.command_buffer_mut().push('/');
        i.char_event('/');
        assert_eq!(i.command_buffer(), "/");
        i.char_event('t');
        assert_eq!(i.command_buffer(), "/t");
        i.key_event(keys::KEY_BACKSPACE, true, false);
        i.key_event(keys::KEY_BACKSPACE, true, true);
        assert_eq!(i.command_buffer(), "");
        i.set_suppress_chat_filter(true);
        i.char_event('T');
        assert_eq!(i.command_buffer(), "T");
        i.set_use_secondary_buffer(true);
        i.char_event('1');
        assert_eq!(i.secondary_buffer(), "1");
    }

    #[test]
    fn slots_scroll_and_digits() {
        let mut i = InputManager::new();
        i.key_event(glfw::KEY_1 + 4, true, false);
        tick(&mut i);
        assert_eq!(i.selected_slot(), 4);
        i.key_event(glfw::KEY_1 + 4, false, false);
        i.scroll(1.0);
        tick(&mut i);
        assert_eq!(i.selected_slot(), 3);
        // La molette n'est remise à 0 qu'en fin de frame : un second tick la revoit.
        tick(&mut i);
        assert_eq!(i.selected_slot(), 2);
        i.end_frame();
        tick(&mut i);
        assert_eq!(i.selected_slot(), 2);
    }

    #[test]
    fn mouse_delta_and_reset() {
        let mut i = InputManager::new();
        i.cursor_moved(10.0, 10.0);
        tick(&mut i);
        assert_eq!(i.mouse_delta(), Vec2::ZERO);
        i.cursor_moved(13.0, 8.0);
        tick(&mut i);
        assert_eq!(i.mouse_delta(), Vec2::new(3.0, -2.0));
        i.reset_mouse();
        i.cursor_moved(100.0, 100.0);
        tick(&mut i);
        assert_eq!(i.mouse_delta(), Vec2::ZERO);
    }

    #[test]
    fn mouse_buttons_map_to_primary_secondary() {
        let mut i = InputManager::new();
        i.mouse_button(glfw::MOUSE_BUTTON_LEFT, true);
        tick(&mut i);
        let p = i.player_input(true, Some(0.2), Some(80.0));
        assert!(p.is_just_pressed(GameAction::Primary));
        assert_eq!(p.sensitivity, Some(0.2));
    }

    #[test]
    fn rebind_captures_next_key_only() {
        let mut i = InputManager::new();
        i.start_rebind();
        i.set_typing_mode(true);
        i.key_event(keys::KEY_BACKSPACE, true, false);
        assert_eq!(i.take_rebind(), Some(keys::KEY_BACKSPACE));
        assert_eq!(i.take_rebind(), None);
        assert!(!i.is_rebinding());
    }

    #[test]
    fn open_chat_keys() {
        for k in [glfw::KEY_T, keys::KEY_SLASH, glfw::KEY_KP_DIVIDE] {
            let mut i = InputManager::new();
            i.key_event(k, true, false);
            tick(&mut i);
            assert!(i.is_just_pressed(Action::OpenChat));
        }
    }
}
