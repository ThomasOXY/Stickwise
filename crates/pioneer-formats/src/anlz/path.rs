use super::{Section, tag};
use crate::error::{Bytes, Error, Result};

/// `PPTH`: the audio file path as UTF-16BE with a trailing NUL.
pub fn parse_path(s: &Section) -> Result<String> {
    let len = Bytes(&s.header).u32be(0)? as usize;
    let data = s
        .body
        .get(..len)
        .ok_or_else(|| Error::Invalid("PPTH shorter than its length".into()))?;
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    Ok(String::from_utf16_lossy(&units))
}

pub fn build_path(path: &str) -> Section {
    let mut body: Vec<u8> = path.encode_utf16().flat_map(u16::to_be_bytes).collect();
    body.extend_from_slice(&[0, 0]);
    Section::new(tag::PATH, (body.len() as u32).to_be_bytes().to_vec(), body)
}
