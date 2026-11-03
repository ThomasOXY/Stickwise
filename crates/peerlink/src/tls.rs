//! TLS 1.3 between two Macs, each showing its own certificate. No
//! certificate authority vouches for them: any certificate is let through
//! here (the handshake still proves the other side holds its key), and
//! [`crate::Node`] decides by the certificate's id whom it trusts.

use std::net::TcpStream;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, DistinguishedName, Error, ServerConfig,
    ServerConnection, SignatureScheme, StreamOwned,
};

use crate::identity::{Identity, cert_id};

#[derive(Debug)]
struct AnyCert(Arc<CryptoProvider>);

impl AnyCert {
    fn tls13(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(message, cert, dss, &self.0.signature_verification_algorithms)
    }

    fn schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.2 is not used".into()))
    }

    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        self.tls13(m, c, d)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes()
    }
}

impl ClientCertVerifier for AnyCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: UnixTime) -> Result<ClientCertVerified, Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("TLS 1.2 is not used".into()))
    }

    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, Error> {
        self.tls13(m, c, d)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.schemes()
    }
}

pub(crate) struct Configs {
    pub server: Arc<ServerConfig>,
    pub client: Arc<ClientConfig>,
}

pub(crate) fn configs(me: &Identity) -> Result<Configs, Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(AnyCert(provider.clone()));
    let server = ServerConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(verifier.clone())
        .with_single_cert(vec![me.cert.clone()], me.key())?;
    let client = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(vec![me.cert.clone()], me.key())?;
    Ok(Configs { server: Arc::new(server), client: Arc::new(client) })
}

pub(crate) enum Stream {
    Server(StreamOwned<ServerConnection, TcpStream>),
    Client(StreamOwned<ClientConnection, TcpStream>),
}

impl Stream {
    pub fn accept(config: Arc<ServerConfig>, sock: TcpStream) -> std::io::Result<(Self, String)> {
        let conn = ServerConnection::new(config).map_err(std::io::Error::other)?;
        let mut s = StreamOwned::new(conn, sock);
        while s.conn.is_handshaking() {
            s.conn.complete_io(&mut s.sock)?;
        }
        let id = peer_id(s.conn.peer_certificates())?;
        Ok((Stream::Server(s), id))
    }

    pub fn connect(config: Arc<ClientConfig>, sock: TcpStream) -> std::io::Result<(Self, String)> {
        let name = ServerName::try_from("stickwise.local").map_err(std::io::Error::other)?;
        let conn = ClientConnection::new(config, name).map_err(std::io::Error::other)?;
        let mut s = StreamOwned::new(conn, sock);
        while s.conn.is_handshaking() {
            s.conn.complete_io(&mut s.sock)?;
        }
        let id = peer_id(s.conn.peer_certificates())?;
        Ok((Stream::Client(s), id))
    }

    pub fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Server(s) => &s.sock,
            Stream::Client(s) => &s.sock,
        }
    }
}

fn peer_id(certs: Option<&[CertificateDer<'_>]>) -> std::io::Result<String> {
    certs
        .and_then(|c| c.first())
        .map(|c| cert_id(c))
        .ok_or_else(|| std::io::Error::other("the other side showed no certificate"))
}

impl std::io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Server(s) => s.read(buf),
            Stream::Client(s) => s.read(buf),
        }
    }
}

impl std::io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Server(s) => s.write(buf),
            Stream::Client(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Server(s) => s.flush(),
            Stream::Client(s) => s.flush(),
        }
    }
}
