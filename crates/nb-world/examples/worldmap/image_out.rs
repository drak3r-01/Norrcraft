//! Écriture d'images RGB sans dépendance : PNG (deflate « stocké ») ou PPM selon l'extension.

use std::io::Write;

fn crc32_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, e) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *e = c;
    }
    t
}

fn crc32(table: &[u32; 256], parts: &[&[u8]]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for part in parts {
        for &b in *part {
            c = table[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
        }
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &d in data {
        a = (a + u32::from(d)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, table: &[u32; 256], kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(table, &[kind, data]).to_be_bytes());
}

/// Encode une image RGB 8 bits en PNG.
pub fn encode_png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), (width * height * 3) as usize);
    let table = crc32_table();
    let mut raw = Vec::with_capacity(rgb.len() + height as usize);
    for row in rgb.chunks((width * 3) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, &table, b"IHDR", &ihdr);
    chunk(&mut out, &table, b"IDAT", &z);
    chunk(&mut out, &table, b"IEND", &[]);
    out
}

/// Écrit l'image : PNG si le nom finit par `.png`, sinon PPM binaire (P6).
pub fn write_image(path: &str, width: u32, height: u32, rgb: &[u8]) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    if path.to_lowercase().ends_with(".png") {
        f.write_all(&encode_png(width, height, rgb))
    } else {
        write!(f, "P6\n{width} {height}\n255\n")?;
        f.write_all(rgb)
    }
}
