//! Façade du moteur audio : un handle léger et non bloquant que le jeu appelle depuis son
//! thread principal. Le mixage tourne dans un thread dédié (ou n'existe pas du tout si aucun
//! périphérique n'est disponible : le jeu continue en silence).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::Sender;

use crate::ambience::AmbienceTargets;
use crate::bank::Bank;
use crate::mixer::{Cmd, Environment, Listener, Mixer, PlayReq, Volumes};
use crate::music::{MusicMode, Theme};
use crate::output::{self, OUTPUT_RATE};

/// Options de démarrage.
#[derive(Clone, Debug, Default)]
pub struct AudioOptions {
    /// Dossier de fichiers `.wav` remplaçant les sons procéduraux (voir [`crate::Sfx::file_stem`]).
    pub sounds_dir: Option<PathBuf>,
}

enum Sink {
    Off,
    Device(Sender<Cmd>),
    Record(Arc<Mutex<Vec<Cmd>>>),
}

/// Le moteur audio vu par le jeu.
pub struct AudioEngine {
    sink: Sink,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for AudioEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.sink {
            Sink::Off => "off",
            Sink::Device(_) => "device",
            Sink::Record(_) => "record",
        };
        write!(f, "AudioEngine({kind})")
    }
}

impl AudioEngine {
    /// Moteur muet (tests, mode sans tête, aucun périphérique) : toutes les commandes sont ignorées.
    pub fn disabled() -> Self {
        Self {
            sink: Sink::Off,
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        }
    }

    /// Moteur qui mémorise les commandes au lieu de les jouer (tests de la logique de jeu).
    pub fn recording() -> Self {
        Self {
            sink: Sink::Record(Arc::new(Mutex::new(Vec::new()))),
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        }
    }

    /// Démarre la sortie audio ; en cas d'échec le message est affiché et le moteur est muet.
    pub fn start(opts: &AudioOptions) -> Self {
        match Self::try_start(opts) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("audio désactivé : {e}");
                Self::disabled()
            }
        }
    }

    pub fn try_start(opts: &AudioOptions) -> Result<Self, String> {
        let bank = Bank::new();
        let (tx, rx) = crossbeam_channel::unbounded();
        let mixer = Mixer::new(OUTPUT_RATE as f32, bank.clone(), rx);
        let stop = Arc::new(AtomicBool::new(false));
        let out = output::spawn(mixer, stop.clone())?;
        // Génération des sons en tâche de fond : le jeu démarre sans attendre.
        let dir = opts.sounds_dir.clone();
        let warm = std::thread::Builder::new()
            .name("nb-audio-synth".into())
            .spawn(move || bank.build_all(dir.as_deref()))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sink: Sink::Device(tx),
            stop,
            threads: vec![out, warm],
        })
    }

    /// Le moteur fait-il quelque chose (périphérique ouvert ou enregistrement) ?
    pub fn is_enabled(&self) -> bool {
        !matches!(self.sink, Sink::Off)
    }

    /// Un périphérique est-il ouvert ?
    pub fn is_active(&self) -> bool {
        matches!(self.sink, Sink::Device(_))
    }

    fn send(&self, c: Cmd) {
        match &self.sink {
            Sink::Off => {}
            Sink::Device(tx) => {
                let _ = tx.send(c);
            }
            Sink::Record(v) => {
                if let Ok(mut v) = v.lock() {
                    v.push(c);
                }
            }
        }
    }

    pub fn play(&self, req: PlayReq) {
        self.send(Cmd::Play(req));
    }

    pub fn set_listener(&self, l: Listener) {
        self.send(Cmd::Listener(l));
    }

    pub fn set_volumes(&self, v: Volumes) {
        self.send(Cmd::Volumes(v));
    }

    pub fn set_environment(&self, e: Environment) {
        self.send(Cmd::Environment(e));
    }

    pub fn set_ambience(&self, a: AmbienceTargets) {
        self.send(Cmd::Ambience(a));
    }

    pub fn set_theme(&self, t: Theme) {
        self.send(Cmd::Theme(t));
    }

    pub fn set_music_mode(&self, m: MusicMode) {
        self.send(Cmd::MusicMode(m));
    }

    pub fn stop_all(&self) {
        self.send(Cmd::StopAll);
    }

    /// Commandes mémorisées (moteur [`Self::recording`] seulement).
    pub fn take_recorded(&self) -> Vec<Cmd> {
        match &self.sink {
            Sink::Record(v) => v
                .lock()
                .map(|mut v| std::mem::take(&mut *v))
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfx::{Bus, Sfx};

    #[test]
    fn disabled_engine_ignores_everything() {
        let e = AudioEngine::disabled();
        e.play(PlayReq::new(Sfx::UiClick, Bus::Effects));
        e.set_volumes(Volumes::default());
        assert!(!e.is_active());
        assert!(e.take_recorded().is_empty());
    }

    #[test]
    fn recording_engine_keeps_commands_in_order() {
        let e = AudioEngine::recording();
        e.set_theme(Theme::Cave);
        e.play(PlayReq::new(Sfx::UiClick, Bus::Effects));
        let r = e.take_recorded();
        assert_eq!(r.len(), 2);
        assert!(matches!(r[0], Cmd::Theme(Theme::Cave)));
        assert!(matches!(r[1], Cmd::Play(_)));
        assert!(e.take_recorded().is_empty());
    }

    #[test]
    fn start_never_panics_without_a_device() {
        // Sans carte son (CI, Docker) le moteur retombe en mode muet.
        let e = AudioEngine::start(&AudioOptions::default());
        e.play(PlayReq::new(Sfx::UiClick, Bus::Effects));
    }
}
