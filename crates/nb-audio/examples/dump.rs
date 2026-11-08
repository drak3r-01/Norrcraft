//! Écrit tous les sons procéduraux et un extrait de musique/ambiance en `.wav`, pour les écouter
//! ou s'en servir de modèles : `cargo run -p nb-audio --example dump -- <dossier>`.
//! Les fichiers `sfx/<nom>.wav` peuvent être copiés tels quels dans le dossier `sounds/` du jeu
//! pour remplacer un son (ou servir de point de départ à un vrai enregistrement).

use std::io::Write;
use std::path::Path;

use nb_audio::bank::all_sfx;
use nb_audio::synth::{render, RATE};
use nb_audio::{AmbienceTargets, Cmd, MusicMode, Theme, Volumes};

fn write_wav(path: &Path, rate: u32, channels: u16, data: &[f32]) -> std::io::Result<()> {
    let pcm: Vec<u8> = data
        .iter()
        .flat_map(|&s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + pcm.len() as u32).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&rate.to_le_bytes())?;
    f.write_all(&(rate * u32::from(channels) * 2).to_le_bytes())?;
    f.write_all(&(channels * 2).to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&(pcm.len() as u32).to_le_bytes())?;
    f.write_all(&pcm)
}

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "audio-dump".into());
    let sfx_dir = Path::new(&dir).join("sfx");
    std::fs::create_dir_all(&sfx_dir)?;
    for sfx in all_sfx() {
        let stem = sfx.file_stem();
        for v in 0..sfx.variants() {
            let name = if v == 0 {
                format!("{stem}.wav")
            } else {
                format!("{stem}_{v}.wav")
            };
            write_wav(&sfx_dir.join(name), RATE as u32, 1, &render(sfx, v))?;
        }
    }
    let themes = [
        (Theme::Menu, "menu"),
        (Theme::Meadow, "meadow"),
        (Theme::Forest, "forest"),
        (Theme::Desert, "desert"),
        (Theme::Snow, "snow"),
        (Theme::Ocean, "ocean"),
        (Theme::Cave, "cave"),
        (Theme::Night, "night"),
        (Theme::Volcano, "volcano"),
        (Theme::Mystic, "mystic"),
    ];
    for (theme, name) in themes {
        let (tx, rx) = crossbeam_channel::unbounded();
        let bank = nb_audio::bank::Bank::new();
        bank.build_all(None);
        let mut mixer = nb_audio::Mixer::new(RATE, bank, rx);
        tx.send(Cmd::Theme(theme)).unwrap();
        tx.send(Cmd::MusicMode(MusicMode::Continuous)).unwrap();
        tx.send(Cmd::Volumes(Volumes {
            music: 1.0,
            ..Default::default()
        }))
        .unwrap();
        tx.send(Cmd::Ambience(AmbienceTargets {
            wind: if matches!(theme, Theme::Snow | Theme::Desert) {
                0.6
            } else {
                0.15
            },
            cave: if theme == Theme::Cave { 1.0 } else { 0.0 },
            water: if theme == Theme::Ocean { 0.8 } else { 0.0 },
            birds: if matches!(theme, Theme::Meadow | Theme::Forest) {
                0.8
            } else {
                0.0
            },
            crickets: if theme == Theme::Night { 0.8 } else { 0.0 },
            ..Default::default()
        }))
        .unwrap();
        let mut out = vec![0.0f32; RATE as usize * 2 * 45];
        mixer.render(&mut out);
        write_wav(
            &Path::new(&dir).join(format!("mix_{name}.wav")),
            RATE as u32,
            2,
            &out,
        )?;
    }
    println!("écrit dans {dir}");
    Ok(())
}
