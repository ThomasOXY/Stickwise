//! Who this Mac is to the others: a key and a self-signed certificate made
//! on first start. A Mac's id is the hash of its certificate, so it cannot
//! be claimed without the key.

use std::io;
use std::path::Path;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub struct Identity {
    pub cert: CertificateDer<'static>,
    key: Vec<u8>,
    pub id: String,
}

impl Identity {
    /// The identity kept in `dir`, made there if there is none.
    pub fn load_or_create(dir: &Path) -> io::Result<Self> {
        let (cert_path, key_path) = (dir.join("cert.der"), dir.join("key.der"));
        if let (Ok(cert), Ok(key)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
            return Ok(Self::new(cert, key));
        }
        std::fs::create_dir_all(dir)?;
        let made = rcgen::generate_simple_self_signed(vec!["stickwise.local".to_string()]).map_err(io::Error::other)?;
        let (cert, key) = (made.cert.der().to_vec(), made.signing_key.serialize_der());
        write_private(&key_path, &key)?;
        std::fs::write(&cert_path, &cert)?;
        Ok(Self::new(cert, key))
    }

    fn new(cert: Vec<u8>, key: Vec<u8>) -> Self {
        let id = cert_id(&cert);
        Identity { cert: CertificateDer::from(cert), key, id }
    }

    pub(crate) fn key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone()))
    }
}

fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(data)?;
    f.sync_all()
}

/// A Mac's id: the first 128 bits of BLAKE3 of its certificate, hex.
pub fn cert_id(der: &[u8]) -> String {
    blake3::hash(der).to_hex()[..32].to_string()
}

/// Six digits both Macs show while pairing. They match only if each sees
/// the other's real certificate, so comparing them rules out anyone in
/// between.
pub fn pairing_code(a: &str, b: &str) -> String {
    let (x, y) = if a < b { (a, b) } else { (b, a) };
    let h = blake3::hash(format!("stickwise-pair\0{x}\0{y}").as_bytes());
    let n = u32::from_be_bytes(h.as_bytes()[..4].try_into().unwrap()) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn made_once_then_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let a = Identity::load_or_create(dir.path()).unwrap();
        let b = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.id.len(), 32);
        let other = Identity::load_or_create(&dir.path().join("x")).unwrap();
        assert_ne!(a.id, other.id);
    }

    #[test]
    fn code_is_the_same_both_ways() {
        let c = pairing_code("aa", "bb");
        assert_eq!(c, pairing_code("bb", "aa"));
        assert_eq!(c.len(), 7);
        assert_ne!(c, pairing_code("aa", "bc"));
    }
}
