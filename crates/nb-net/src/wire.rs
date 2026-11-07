//! Lecture et écriture binaires (little-endian). Toute lecture est bornée : un paquet tronqué
//! ou mal formé renvoie une [`WireError`], jamais une panique.

use std::fmt;

/// Erreur de décodage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    /// Fin des données avant la fin du champ.
    Truncated,
    /// Chaîne non UTF-8.
    BadString,
    /// Étiquette de message inconnue.
    BadTag(u8),
    /// Longueur annoncée au-delà des limites.
    TooLong(usize),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireError::Truncated => write!(f, "message tronqué"),
            WireError::BadString => write!(f, "texte invalide"),
            WireError::BadTag(t) => write!(f, "type de message inconnu ({t})"),
            WireError::TooLong(n) => write!(f, "longueur excessive ({n})"),
        }
    }
}

impl std::error::Error for WireError {}

/// Écrivain dans un `Vec<u8>`.
#[derive(Debug, Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new(tag: u8) -> Self {
        Self { buf: vec![tag] }
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(u8::from(v))
    }
    /// Texte : longueur `u32` puis octets UTF-8.
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self
    }
    /// Octets bruts précédés de leur longueur `u32`.
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.u32(b.len() as u32);
        self.buf.extend_from_slice(b);
        self
    }
}

/// Lecteur borné sur une tranche.
#[derive(Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

/// Limite d'un tableau ou d'un texte décodé (protège contre les longueurs forgées).
pub const MAX_ITEMS: usize = 1 << 20;

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.pos.checked_add(n).ok_or(WireError::Truncated)?;
        let s = self.data.get(self.pos..end).ok_or(WireError::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 octets"),
        ))
    }
    pub fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 octets"),
        ))
    }
    pub fn i32(&mut self) -> Result<i32, WireError> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("4 octets"),
        ))
    }
    pub fn i64(&mut self) -> Result<i64, WireError> {
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().expect("8 octets"),
        ))
    }
    pub fn f32(&mut self) -> Result<f32, WireError> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().expect("4 octets"),
        ))
    }
    pub fn f64(&mut self) -> Result<f64, WireError> {
        Ok(f64::from_le_bytes(
            self.take(8)?.try_into().expect("8 octets"),
        ))
    }
    pub fn bool(&mut self) -> Result<bool, WireError> {
        Ok(self.u8()? != 0)
    }
    /// Longueur d'un tableau, bornée par [`MAX_ITEMS`] et par les octets restants.
    pub fn len(&mut self, min_item_size: usize) -> Result<usize, WireError> {
        let n = self.u32()? as usize;
        if n > MAX_ITEMS || n.saturating_mul(min_item_size.max(1)) > self.remaining() {
            return Err(WireError::TooLong(n));
        }
        Ok(n)
    }
    pub fn str(&mut self) -> Result<String, WireError> {
        let n = self.len(1)?;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| WireError::BadString)
    }
    pub fn bytes(&mut self) -> Result<Vec<u8>, WireError> {
        let n = self.len(1)?;
        Ok(self.take(n)?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut w = Writer::new(7);
        w.u8(1)
            .u16(65535)
            .u32(4_000_000_000)
            .i32(-5)
            .i64(i64::MIN)
            .f32(1.5)
            .f64(-2.25)
            .bool(true)
            .str("héllo")
            .bytes(&[1, 2, 3]);
        let mut r = Reader::new(&w.buf);
        assert_eq!(r.u8().unwrap(), 7);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u16().unwrap(), 65535);
        assert_eq!(r.u32().unwrap(), 4_000_000_000);
        assert_eq!(r.i32().unwrap(), -5);
        assert_eq!(r.i64().unwrap(), i64::MIN);
        assert_eq!(r.f32().unwrap(), 1.5);
        assert_eq!(r.f64().unwrap(), -2.25);
        assert!(r.bool().unwrap());
        assert_eq!(r.str().unwrap(), "héllo");
        assert_eq!(r.bytes().unwrap(), vec![1, 2, 3]);
        assert_eq!(r.remaining(), 0);
        assert_eq!(r.u8(), Err(WireError::Truncated));
    }

    #[test]
    fn forged_lengths_are_rejected() {
        let mut w = Writer::default();
        w.u32(u32::MAX);
        assert!(matches!(
            Reader::new(&w.buf).str(),
            Err(WireError::TooLong(_))
        ));
        let mut w = Writer::default();
        w.u32(10).u8(1);
        assert!(Reader::new(&w.buf).bytes().is_err());
        let mut w = Writer::default();
        w.u32(2).u8(0xff).u8(0xfe);
        assert_eq!(Reader::new(&w.buf).str(), Err(WireError::BadString));
    }
}
