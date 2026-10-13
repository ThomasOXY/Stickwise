use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unexpected end of data at offset {offset:#x} (need {need} bytes)")]
    Eof { offset: usize, need: usize },
    #[error("bad magic at offset {offset:#x}: expected {expected:?}")]
    Magic {
        offset: usize,
        expected: &'static str,
    },
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Bounds-checked little/big-endian reads over a byte slice.
pub(crate) struct Bytes<'a>(pub &'a [u8]);

impl Bytes<'_> {
    pub fn slice(&self, offset: usize, len: usize) -> Result<&[u8]> {
        offset
            .checked_add(len)
            .and_then(|end| self.0.get(offset..end))
            .ok_or(Error::Eof { offset, need: len })
    }

    pub fn u8(&self, o: usize) -> Result<u8> {
        Ok(self.slice(o, 1)?[0])
    }

    pub fn u16le(&self, o: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.slice(o, 2)?.try_into().unwrap()))
    }

    pub fn u32le(&self, o: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.slice(o, 4)?.try_into().unwrap()))
    }

    pub fn u16be(&self, o: usize) -> Result<u16> {
        Ok(u16::from_be_bytes(self.slice(o, 2)?.try_into().unwrap()))
    }

    pub fn u32be(&self, o: usize) -> Result<u32> {
        Ok(u32::from_be_bytes(self.slice(o, 4)?.try_into().unwrap()))
    }
}
