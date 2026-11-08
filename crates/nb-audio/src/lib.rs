//! nb-audio — moteur de son de Norrblock.
//!
//! * **Spatialisation 3D** : atténuation avec la distance, panoramique à puissance constante
//!   selon l'orientation de la tête, assombrissement des sources lointaines ou dans le dos.
//! * **Mixage** : volume général + quatre canaux (musique, ambiance, effets, créatures),
//!   réverbération de grotte, étouffement sous l'eau, limiteur doux.
//! * **Sons procéduraux** (placeholders libres de droits, aucun échantillon embarqué) :
//!   casse / pose / pas pour 14 matériaux, voix des créatures, interface, oiseaux, gouttes…
//!   Un fichier `.wav` posé dans le dossier des sons remplace n'importe lequel d'entre eux.
//! * **Ambiances** (vent, grotte, vagues, grillons) et **musique générative** par thème de biome.
//!
//! Le calcul ([`Mixer`]) est pur et testable ; seule la sortie ([`output`]) touche au système
//! (waveOut sous Windows). Le jeu parle au moteur via le handle non bloquant [`AudioEngine`].

pub mod ambience;
pub mod bank;
pub mod dsp;
pub mod engine;
pub mod mixer;
pub mod music;
pub mod output;
pub mod sfx;
pub mod synth;

pub use ambience::AmbienceTargets;
pub use engine::{AudioEngine, AudioOptions};
pub use mixer::{Cmd, Environment, Listener, Mixer, PlayReq, Volumes, MAX_VOICES};
pub use music::{MusicMode, Theme};
pub use sfx::{voice_for_mob, Bus, Material, MobCall, Sfx, Voice};
