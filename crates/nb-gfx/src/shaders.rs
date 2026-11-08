//! SPIR-V précompilés (`shaders/spv`, générés par `scripts/compile-shaders.sh` depuis
//! `shaders/glsl450`).

macro_rules! spv {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/spv/",
            $name,
            ".spv"
        ))
    };
}

pub const MAIN_VERT: &[u8] = spv!("main.vert");
pub const MAIN_FRAG: &[u8] = spv!("main.frag");
pub const FLUID_FRAG: &[u8] = spv!("fluid.frag");
pub const SHADOW_VERT: &[u8] = spv!("shadow.vert");
pub const SHADOW_FRAG: &[u8] = spv!("shadow.frag");
pub const HL_VERT: &[u8] = spv!("hl.vert");
pub const HL_FRAG: &[u8] = spv!("hl.frag");
pub const QUAD_VERT: &[u8] = spv!("quad.vert");
pub const BLUR_FRAG: &[u8] = spv!("blur.frag");
pub const PP_FRAG: &[u8] = spv!("pp.frag");
pub const MOB_VERT: &[u8] = spv!("mob.vert");
pub const MOB_FRAG: &[u8] = spv!("mob.frag");
pub const PARTICLE_VERT: &[u8] = spv!("particle.vert");
pub const PARTICLE_FRAG: &[u8] = spv!("particle.frag");

/// Convertit un binaire SPIR-V en mots (copie alignée).
pub fn words(bytes: &[u8]) -> Vec<u32> {
    let (words, rest) = bytes.as_chunks::<4>();
    assert!(rest.is_empty(), "SPIR-V de taille invalide");
    words.iter().map(|c| u32::from_le_bytes(*c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn spirv_magic() {
        for s in [
            MAIN_VERT,
            MAIN_FRAG,
            FLUID_FRAG,
            SHADOW_VERT,
            SHADOW_FRAG,
            HL_VERT,
            HL_FRAG,
            QUAD_VERT,
            BLUR_FRAG,
            PP_FRAG,
            MOB_VERT,
            MOB_FRAG,
            PARTICLE_VERT,
            PARTICLE_FRAG,
        ] {
            assert_eq!(words(s)[0], 0x0723_0203);
        }
    }

    /// Les SPIR-V commités correspondent aux sources GLSL (sinon relancer
    /// `scripts/compile-shaders.sh`).
    #[test]
    fn spirv_is_up_to_date() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shaders");
        let manifest = std::fs::read_to_string(root.join("spv/SOURCES.sha256")).unwrap();
        let mut count = 0;
        for line in manifest.lines().filter(|l| !l.trim().is_empty()) {
            let (hash, name) = line.split_once("  ").expect("ligne sha256sum");
            let src = std::fs::read(root.join("glsl450").join(name.trim())).unwrap();
            let got: String = Sha256::digest(&src)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(got, hash, "{name} modifié : recompiler les shaders");
            count += 1;
        }
        let sources = std::fs::read_dir(root.join("glsl450")).unwrap().count();
        assert_eq!(count, sources, "source GLSL absente du manifeste");
    }
}
