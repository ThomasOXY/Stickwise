//! DeviceSQL strings as stored in `export.pdb` rows.
//!
//! Three encodings exist:
//! * short ASCII: one flag byte `((len + 1) << 1) | 1`, then `len` bytes;
//! * long ASCII: `0x40`, u16 total length, pad byte, data;
//! * UTF-16LE: `0x90`, u16 total length, pad byte, data.
//!
//! rekordbox uses short ASCII up to 126 bytes, long ASCII for longer ASCII
//! strings (seen for long file paths and comments) and UTF-16LE for anything
//! non-ASCII; [`encode`] does the same.

use crate::error::{Bytes, Error, Result};

/// Encode a string the way rekordbox does.
pub fn encode(s: &str) -> Vec<u8> {
    if s.is_ascii() && s.len() < 0x7f {
        let mut out = Vec::with_capacity(s.len() + 1);
        out.push((((s.len() + 1) as u8) << 1) | 1);
        out.extend_from_slice(s.as_bytes());
        out
    } else if s.is_ascii() {
        let mut out = Vec::with_capacity(s.len() + 4);
        out.push(0x40);
        out.extend_from_slice(&((s.len() + 4) as u16).to_le_bytes());
        out.push(0);
        out.extend_from_slice(s.as_bytes());
        out
    } else {
        let data: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut out = Vec::with_capacity(data.len() + 4);
        out.push(0x90);
        out.extend_from_slice(&((data.len() + 4) as u16).to_le_bytes());
        out.push(0);
        out.extend_from_slice(&data);
        out
    }
}

/// Decode the string at `offset`, returning it with its encoded length.
pub fn decode(buf: &[u8], offset: usize) -> Result<(String, usize)> {
    let b = Bytes(buf);
    let flag = b.u8(offset)?;
    if flag & 1 == 1 {
        let n = (flag >> 1) as usize;
        if n == 0 {
            return Ok((String::new(), 1));
        }
        let body = b.slice(offset + 1, n - 1)?;
        return Ok((String::from_utf8_lossy(body).into_owned(), n));
    }
    let n = b.u16le(offset + 1)? as usize;
    if n < 4 {
        return Err(Error::Invalid(format!(
            "string at {offset:#x} has length {n}"
        )));
    }
    let body = b.slice(offset + 4, n - 4)?;
    let s = match flag {
        0x40 => String::from_utf8_lossy(body).into_owned(),
        0x90 => {
            let units: Vec<u16> = body
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => {
            return Err(Error::Invalid(format!(
                "unknown string kind {flag:#x} at {offset:#x}"
            )));
        }
    };
    Ok((s, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ascii() {
        assert_eq!(encode(""), vec![0x03]);
        assert_eq!(encode("1A"), vec![0x07, b'1', b'A']);
        assert_eq!(
            decode(&[0x0b, b'P', b'i', b'n', b'k'], 0).unwrap(),
            ("Pink".into(), 5)
        );
    }

    #[test]
    fn utf16_round_trip() {
        for s in ["", "Techno", "Příliš žluťoučký kůň", "22. 盾", "🅴"] {
            let e = encode(s);
            assert_eq!(decode(&e, 0).unwrap(), (s.to_string(), e.len()));
        }
    }

    #[test]
    fn long_ascii() {
        let s = "x".repeat(200);
        let e = encode(&s);
        assert_eq!(&e[..4], &[0x40, 204, 0, 0]);
        assert_eq!(decode(&e, 0).unwrap().0, s);
    }
}
