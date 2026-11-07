//! Stockage des chunks (`IChunkStorage`, `InMemoryChunkStorage`, `FileChunkStorage`).
//!
//! Seuls les voxels sont persistés. Format de fichier (nouveau, versionné ; aucune
//! compatibilité avec les `.bin` du Java) — `chunks/c_<cx>_<cz>.nbc` :
//!
//! | octets | contenu |
//! |---|---|
//! | 4 | magique `NBCK` |
//! | 2 | version (u16 LE) = 1 |
//! | 1 | layout : 0 = `x + 16·y + 2048·z` |
//! | 1 | compression : 0 = brut, 1 = RLE (paires `(longueur 1..=255, id)`) |
//! | 4 | longueur de la charge utile (u32 LE) |
//! | n | charge utile (32 768 identifiants une fois décompressée) |

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use nb_core::{ChunkPos, Error, Result};
use nb_data::BlockId;

use crate::chunk::CHUNK_VOLUME;

/// Voxels d'un chunk, ordre `x + 16·y + 2048·z`.
pub type Voxels = Box<[BlockId; CHUNK_VOLUME]>;

/// Magique des fichiers de chunk.
pub const CHUNK_MAGIC: [u8; 4] = *b"NBCK";
/// Version courante du format de chunk.
pub const CHUNK_FORMAT_VERSION: u16 = 1;
const LAYOUT_JAVA: u8 = 0;
const COMPRESSION_RAW: u8 = 0;
const COMPRESSION_RLE: u8 = 1;
const HEADER_LEN: usize = 12;

/// Persistance des voxels d'un chunk.
pub trait ChunkStorage: Send + Sync {
    /// Persiste les voxels (pas la lumière).
    fn save_chunk(&self, pos: ChunkPos, voxels: &[BlockId; CHUNK_VOLUME]) -> Result<()>;
    /// `Ok(None)` si absent. Une erreur conduit l'appelant à régénérer le chunk (comme le Java).
    fn load_chunk(&self, pos: ChunkPos) -> Result<Option<Voxels>>;
    fn has_chunk(&self, pos: ChunkPos) -> bool;
}

fn boxed(v: &[BlockId; CHUNK_VOLUME]) -> Voxels {
    Box::new(*v)
}

/// Stockage en mémoire (monde du menu) ; copie les voxels à l'enregistrement.
#[derive(Debug, Default)]
pub struct InMemoryChunkStorage {
    chunks: Mutex<HashMap<ChunkPos, Voxels>>,
}

impl InMemoryChunkStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Oublie un chunk (monde client : donnée périmée).
    pub fn remove(&self, pos: ChunkPos) {
        self.lock().remove(&pos);
    }

    /// Positions stockées.
    pub fn positions(&self) -> Vec<ChunkPos> {
        self.lock().keys().copied().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ChunkPos, Voxels>> {
        self.chunks.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ChunkStorage for InMemoryChunkStorage {
    fn save_chunk(&self, pos: ChunkPos, voxels: &[BlockId; CHUNK_VOLUME]) -> Result<()> {
        self.lock().insert(pos, boxed(voxels));
        Ok(())
    }

    fn load_chunk(&self, pos: ChunkPos) -> Result<Option<Voxels>> {
        Ok(self.lock().get(&pos).map(|v| boxed(v)))
    }

    fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.lock().contains_key(&pos)
    }
}

/// Stockage disque : un fichier `.nbc` par chunk dans `<monde>/chunks`.
#[derive(Debug, Clone)]
pub struct FileChunkStorage {
    chunks_dir: PathBuf,
}

impl FileChunkStorage {
    /// Crée `world_dir/chunks` si besoin.
    pub fn new(world_dir: &Path) -> Result<Self> {
        let chunks_dir = world_dir.join("chunks");
        fs::create_dir_all(&chunks_dir).map_err(|e| Error::io(&chunks_dir, e))?;
        Ok(Self { chunks_dir })
    }

    pub fn chunks_dir(&self) -> &Path {
        &self.chunks_dir
    }

    /// Chemin du fichier d'un chunk.
    pub fn chunk_path(&self, pos: ChunkPos) -> PathBuf {
        self.chunks_dir.join(chunk_file_name(pos))
    }
}

/// `c_<cx>_<cz>.nbc`.
pub fn chunk_file_name(pos: ChunkPos) -> String {
    format!("c_{}_{}.nbc", pos.x, pos.z)
}

/// Compression RLE : suite de paires `(longueur, id)`, longueur dans `1..=255`.
fn rle_encode(voxels: &[BlockId]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut iter = voxels.iter().copied().peekable();
    while let Some(id) = iter.next() {
        let mut run: u8 = 1;
        while run < u8::MAX && iter.peek() == Some(&id) {
            iter.next();
            run += 1;
        }
        out.push(run);
        out.push(id);
    }
    out
}

fn rle_decode(payload: &[u8]) -> Result<Vec<u8>> {
    let bad = |m: &str| Error::data("fichier de chunk", m);
    if payload.len() & 1 == 1 {
        return Err(bad("RLE : longueur impaire"));
    }
    let mut out = Vec::with_capacity(CHUNK_VOLUME);
    let runs = payload.iter().step_by(2);
    let ids = payload.iter().skip(1).step_by(2);
    for (&run, &id) in runs.zip(ids) {
        let run = usize::from(run);
        if run == 0 || out.len() + run > CHUNK_VOLUME {
            return Err(bad("RLE : séquence invalide"));
        }
        out.resize(out.len() + run, id);
    }
    Ok(out)
}

/// Sérialise des voxels (compression RLE).
pub fn encode_chunk(voxels: &[BlockId; CHUNK_VOLUME]) -> Vec<u8> {
    let payload = rle_encode(voxels);
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(&CHUNK_MAGIC);
    out.extend_from_slice(&CHUNK_FORMAT_VERSION.to_le_bytes());
    out.push(LAYOUT_JAVA);
    out.push(COMPRESSION_RLE);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

/// Désérialise un fichier de chunk ; vérifie magique, version, layout et taille.
pub fn decode_chunk(bytes: &[u8]) -> Result<Voxels> {
    let bad = |m: &str| Error::data("fichier de chunk", m);
    if bytes.len() < HEADER_LEN || bytes[..4] != CHUNK_MAGIC {
        return Err(bad("en-tête NBCK absent"));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != CHUNK_FORMAT_VERSION {
        return Err(Error::SaveVersion {
            found: u32::from(version),
            expected: u32::from(CHUNK_FORMAT_VERSION),
        });
    }
    if bytes[6] != LAYOUT_JAVA {
        return Err(bad("layout inconnu"));
    }
    let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let payload = bytes
        .get(HEADER_LEN..HEADER_LEN + len)
        .ok_or_else(|| bad("charge utile tronquée"))?;
    let raw = match bytes[7] {
        COMPRESSION_RAW => payload.to_vec(),
        COMPRESSION_RLE => rle_decode(payload)?,
        _ => return Err(bad("compression inconnue")),
    };
    raw.into_boxed_slice()
        .try_into()
        .map_err(|_| bad("taille de voxels incorrecte"))
}

/// Écriture atomique (fichier temporaire puis renommage).
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| Error::io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| Error::io(path, e))
}

impl ChunkStorage for FileChunkStorage {
    fn save_chunk(&self, pos: ChunkPos, voxels: &[BlockId; CHUNK_VOLUME]) -> Result<()> {
        write_atomic(&self.chunk_path(pos), &encode_chunk(voxels))
    }

    fn load_chunk(&self, pos: ChunkPos) -> Result<Option<Voxels>> {
        let path = self.chunk_path(pos);
        match fs::read(&path) {
            Ok(bytes) => decode_chunk(&bytes).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.chunk_path(pos).is_file()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Répertoire temporaire unique, supprimé à la destruction.
    pub(crate) struct TempDir(pub PathBuf);
    impl TempDir {
        pub(crate) fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let p = std::env::temp_dir().join(format!(
                "nb-world-{tag}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample() -> [BlockId; CHUNK_VOLUME] {
        let mut v = [0u8; CHUNK_VOLUME];
        for (i, b) in v.iter_mut().enumerate() {
            *b = (i % 35) as u8;
        }
        v
    }

    #[test]
    fn encode_decode_roundtrip() {
        let v = sample();
        let bytes = encode_chunk(&v);
        assert_eq!(&bytes[..4], b"NBCK");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 1);
        assert_eq!(*decode_chunk(&bytes).unwrap(), v);
        // Un chunk d'air tient en 129 paires RLE.
        assert_eq!(encode_chunk(&[0; CHUNK_VOLUME]).len(), HEADER_LEN + 2 * 129);
        let raw_air = {
            let mut b = encode_chunk(&[0; CHUNK_VOLUME]);
            b.truncate(HEADER_LEN);
            b[7] = COMPRESSION_RAW;
            b[8..12].copy_from_slice(&(CHUNK_VOLUME as u32).to_le_bytes());
            b.extend_from_slice(&[5u8; CHUNK_VOLUME]);
            b
        };
        assert_eq!(*decode_chunk(&raw_air).unwrap(), [5u8; CHUNK_VOLUME]);
    }

    #[test]
    fn decode_rejects_bad_files() {
        let v = sample();
        let mut bytes = encode_chunk(&v);
        assert!(decode_chunk(&bytes[..10]).is_err());
        assert!(decode_chunk(&[0u8; CHUNK_VOLUME]).is_err()); // ancien .bin brut
        bytes[4] = 9;
        assert!(matches!(
            decode_chunk(&bytes),
            Err(Error::SaveVersion { found: 9, .. })
        ));
        // Brut mais trop court.
        let mut raw = Vec::from(CHUNK_MAGIC);
        raw.extend_from_slice(&1u16.to_le_bytes());
        raw.extend_from_slice(&[0, 0]);
        raw.extend_from_slice(&3u32.to_le_bytes());
        raw.extend_from_slice(&[1, 2, 3]);
        assert!(decode_chunk(&raw).is_err());
    }

    #[test]
    fn memory_storage_copies() {
        let s = InMemoryChunkStorage::new();
        let p = ChunkPos::new(-3, 12);
        assert!(!s.has_chunk(p));
        assert!(s.load_chunk(p).unwrap().is_none());
        let v = sample();
        s.save_chunk(p, &v).unwrap();
        assert!(s.has_chunk(p));
        assert_eq!(s.load_chunk(p).unwrap().unwrap()[0], 0);
    }

    #[test]
    fn file_storage_roundtrip() {
        let dir = TempDir::new("storage");
        let s = FileChunkStorage::new(&dir.0).unwrap();
        let p = ChunkPos::new(-3, 12);
        assert!(!s.has_chunk(p));
        assert!(s.load_chunk(p).unwrap().is_none());
        let v = sample();
        s.save_chunk(p, &v).unwrap();
        assert!(dir.0.join("chunks").join("c_-3_12.nbc").is_file());
        assert!(s.has_chunk(p));
        assert_eq!(*s.load_chunk(p).unwrap().unwrap(), v);
        fs::write(s.chunk_path(p), b"garbage").unwrap();
        assert!(s.load_chunk(p).is_err());
    }
}
