//! Banque de sons : échantillons procéduraux générés en tâche de fond, remplaçables par des
//! fichiers `.wav` posés dans un dossier de sons.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::sfx::{Material, MobCall, Sfx, Voice};
use crate::synth::{self, RATE};

/// Un échantillon mono à [`RATE`] Hz.
pub type Sample = Arc<Vec<f32>>;

/// Décode un fichier WAV (PCM 8/16/24/32 bits ou flottant 32 bits, mono ou multicanal) en mono
/// à [`RATE`] Hz.
pub fn decode_wav(bytes: &[u8]) -> Result<Vec<f32>, String> {
    let u16le = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32le = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("pas un fichier WAV".into());
    }
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let size = u32le(i + 4) as usize;
        let body = i + 8;
        let end = (body + size).min(bytes.len());
        if id == b"fmt " && end - body >= 16 {
            fmt = Some((
                u16le(body),
                u16le(body + 2),
                u32le(body + 4),
                u16le(body + 14),
            ));
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
        }
        i = body + size + (size & 1);
    }
    let (tag, channels, rate, bits) = fmt.ok_or("chunk fmt manquant")?;
    let data = data.ok_or("chunk data manquant")?;
    let channels = usize::from(channels.max(1));
    let bytes_per = usize::from(bits / 8);
    if !matches!((tag, bits), (1, 8 | 16 | 24 | 32) | (3, 32)) {
        return Err(format!("format WAV non géré (tag {tag}, {bits} bits)"));
    }
    let sample_at = |k: usize| -> f32 {
        let b = &data[k * bytes_per..(k + 1) * bytes_per];
        match (tag, bits) {
            (1, 8) => (f32::from(b[0]) - 128.0) / 128.0,
            (1, 16) => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0,
            (1, 24) => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            (1, _) => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
            _ => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        }
    };
    let frames = data.len() / (bytes_per * channels);
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0;
        for c in 0..channels {
            acc += sample_at(f * channels + c);
        }
        mono.push(acc / channels as f32);
    }
    if rate == 0 {
        return Err("fréquence nulle".into());
    }
    if (rate as f32 - RATE).abs() < 1.0 {
        return Ok(mono);
    }
    let ratio = rate as f64 / f64::from(RATE);
    let n = (mono.len() as f64 / ratio) as usize;
    Ok((0..n)
        .map(|k| {
            let p = k as f64 * ratio;
            let a = p as usize;
            let t = (p - a as f64) as f32;
            let x0 = mono[a.min(mono.len() - 1)];
            let x1 = mono[(a + 1).min(mono.len() - 1)];
            x0 + (x1 - x0) * t
        })
        .collect())
}

/// Tous les sons connus, dans l'ordre de génération (les plus urgents d'abord).
pub fn all_sfx() -> Vec<Sfx> {
    let mut v = vec![
        Sfx::UiClick,
        Sfx::UiOpen,
        Sfx::UiClose,
        Sfx::UiSlider,
        Sfx::Hit,
        Sfx::PlayerHurt,
        Sfx::PlayerDeath,
        Sfx::Respawn,
        Sfx::Pickup,
        Sfx::Craft,
    ];
    let common = [
        Material::Stone,
        Material::Dirt,
        Material::Grass,
        Material::Wood,
        Material::Sand,
        Material::Gravel,
        Material::Snow,
        Material::Leaves,
    ];
    let rest = Material::ALL.iter().filter(|m| !common.contains(m));
    for &m in common.iter().chain(rest) {
        v.extend([Sfx::Step(m), Sfx::Break(m), Sfx::Place(m), Sfx::Land(m)]);
    }
    v.extend([Sfx::Splash, Sfx::Swim, Sfx::Drip, Sfx::Bird, Sfx::Crackle]);
    for voice in [
        Voice::Grunt,
        Voice::Moo,
        Voice::Bleat,
        Voice::Cluck,
        Voice::Squawk,
        Voice::Growl,
        Voice::Squeak,
        Voice::Croak,
        Voice::Hiss,
        Voice::Chitter,
        Voice::Whale,
        Voice::Bubble,
        Voice::Squelch,
        Voice::Whinny,
    ] {
        for call in [MobCall::Idle, MobCall::Hurt, MobCall::Death] {
            v.push(Sfx::Mob(voice, call));
        }
    }
    v
}

/// Banque partagée entre le thread de génération et le mixeur.
#[derive(Debug, Default)]
pub struct Bank {
    map: RwLock<HashMap<Sfx, Vec<Sample>>>,
}

impl Bank {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Variante `variant % n` du son, si déjà disponible (jamais bloquant plus qu'un verrou court).
    pub fn get(&self, sfx: Sfx, variant: usize) -> Option<Sample> {
        let map = self.map.read().ok()?;
        let list = map.get(&sfx)?;
        list.get(variant % list.len().max(1)).cloned()
    }

    /// Nombre de variantes disponibles.
    pub fn variants(&self, sfx: Sfx) -> usize {
        self.map
            .read()
            .ok()
            .and_then(|m| m.get(&sfx).map(Vec::len))
            .unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.map.read().map_or(0, |m| m.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&self, sfx: Sfx, samples: Vec<Sample>) {
        if samples.is_empty() {
            return;
        }
        if let Ok(mut m) = self.map.write() {
            m.insert(sfx, samples);
        }
    }

    /// Fichiers `<stem>.wav`, `<stem>_1.wav`, `<stem>_2.wav`… du dossier `dir`.
    fn load_overrides(dir: &Path, sfx: Sfx) -> Vec<Sample> {
        let stem = sfx.file_stem();
        let mut out = Vec::new();
        let mut names = vec![format!("{stem}.wav")];
        names.extend((1..=16).map(|i| format!("{stem}_{i}.wav")));
        for n in names {
            let Ok(bytes) = std::fs::read(dir.join(&n)) else {
                continue;
            };
            match decode_wav(&bytes) {
                Ok(s) if !s.is_empty() => out.push(Arc::new(s)),
                Ok(_) => {}
                Err(e) => eprintln!("son {n} ignoré : {e}"),
            }
        }
        out
    }

    /// Génère (ou charge depuis `dir`) un son.
    pub fn build(&self, sfx: Sfx, dir: Option<&Path>) {
        let custom = dir
            .map(|d| Self::load_overrides(d, sfx))
            .unwrap_or_default();
        if !custom.is_empty() {
            self.insert(sfx, custom);
            return;
        }
        let list = (0..sfx.variants())
            .map(|v| Arc::new(synth::render(sfx, v)))
            .collect();
        self.insert(sfx, list);
    }

    /// Génère tout, dans l'ordre de priorité.
    pub fn build_all(&self, dir: Option<&Path>) {
        for sfx in all_sfx() {
            self.build(sfx, dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav16(rate: u32, channels: u16, frames: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = frames.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut v = Vec::new();
        v.extend(b"RIFF");
        v.extend((36 + data.len() as u32).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(channels.to_le_bytes());
        v.extend(rate.to_le_bytes());
        v.extend((rate * u32::from(channels) * 2).to_le_bytes());
        v.extend((channels * 2).to_le_bytes());
        v.extend(16u16.to_le_bytes());
        v.extend(b"data");
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }

    #[test]
    fn decodes_mono_and_downmixes_stereo() {
        let mono = decode_wav(&wav16(44_100, 1, &[0, 16384, -16384])).unwrap();
        assert_eq!(mono.len(), 3);
        assert!((mono[1] - 0.5).abs() < 1e-3);
        let st = decode_wav(&wav16(44_100, 2, &[16384, -16384, 32767, 32767])).unwrap();
        assert_eq!(st.len(), 2);
        assert!(st[0].abs() < 1e-3 && st[1] > 0.99);
    }

    #[test]
    fn resamples_to_engine_rate() {
        let half = decode_wav(&wav16(22_050, 1, &[1000; 100])).unwrap();
        assert!((half.len() as i32 - 200).abs() <= 1);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_wav(b"not a wav file at all").is_err());
    }

    #[test]
    fn bank_serves_variants_and_wraps() {
        let b = Bank::new();
        assert!(b.get(Sfx::UiClick, 0).is_none());
        b.build(Sfx::Step(Material::Stone), None);
        assert_eq!(b.variants(Sfx::Step(Material::Stone)), 5);
        assert!(b.get(Sfx::Step(Material::Stone), 17).is_some());
    }

    #[test]
    fn all_sfx_has_no_duplicates() {
        let v = all_sfx();
        let set: std::collections::HashSet<_> = v.iter().collect();
        assert_eq!(set.len(), v.len());
    }

    #[test]
    fn wav_files_override_procedural_sounds() {
        let dir = std::env::temp_dir().join(format!("nb-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ui_click.wav"), wav16(44_100, 1, &[1000; 50])).unwrap();
        let b = Bank::new();
        b.build(Sfx::UiClick, Some(&dir));
        assert_eq!(b.get(Sfx::UiClick, 0).unwrap().len(), 50);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
