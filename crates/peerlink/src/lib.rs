//! Stickwise on other Macs: finding them on the local network (Bonjour),
//! pairing with a six-digit code, and encrypted connections between Macs
//! that are paired.
//!
//! Each Mac has its own certificate ([`Identity`]); its id is the hash of
//! it. Pairing remembers the other Mac's id after both sides showed the
//! same code. From then on a connection is accepted only from (and made
//! only to) a Mac whose certificate has that id, over TLS 1.3. Messages are
//! JSON frames, each led by its length.

mod identity;
mod tls;

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use identity::{Identity, cert_id, pairing_code};

/// The Bonjour service every Stickwise announces.
pub const SERVICE: &str = "_stickwise._tcp.local.";
/// Version of the messages below; Macs with another one do not talk.
pub const PROTOCOL: u32 = 1;
/// How long a pairing request waits for the other Mac's answer.
pub const PAIR_TIMEOUT: Duration = Duration::from_secs(90);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_FRAME: usize = 64 << 20;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Tls(#[from] rustls::Error),
    #[error(transparent)]
    Mdns(#[from] mdns_sd::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Said to the DJ as it is.
    #[error("{0}")]
    Refused(String),
}

pub type Result<T> = std::result::Result<T, Error>;

fn refused<T>(why: &str) -> Result<T> {
    Err(Error::Refused(why.to_string()))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Msg {
    /// First message of every connection, both ways; `pair` asks to pair.
    Hello { name: String, protocol: u32, pair: bool },
    PairAccepted,
    PairRejected,
    /// Already answering another pairing request.
    Busy,
    /// The connecting Mac is not paired with this one.
    NotPaired,
    Incompatible { protocol: u32 },
    Ping,
    Pong,
}

fn write_frame<T: Serialize>(w: &mut impl Write, m: &T) -> Result<()> {
    let b = serde_json::to_vec(m)?;
    w.write_all(&(b.len() as u32).to_be_bytes())?;
    w.write_all(&b)?;
    w.flush()?;
    Ok(())
}

fn read_frame<T: DeserializeOwned>(r: &mut impl Read) -> Result<T> {
    let mut n = [0u8; 4];
    r.read_exact(&mut n)?;
    let n = u32::from_be_bytes(n) as usize;
    if n > MAX_FRAME {
        return refused("Zpráva z druhého Macu je moc velká.");
    }
    let mut b = vec![0u8; n];
    r.read_exact(&mut b)?;
    Ok(serde_json::from_slice(&b)?)
}

/// A connection between two paired Macs. Messages are any JSON the two
/// sides agree on ([`Msg::Ping`] / [`Msg::Pong`] are answered by default);
/// raw bytes (files) go through `Read` / `Write` between them.
pub struct Session {
    stream: tls::Stream,
    pub peer_id: String,
    pub peer_name: String,
}

impl Read for Session {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for Session {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl Session {
    pub fn send<T: Serialize>(&mut self, m: &T) -> Result<()> {
        write_frame(&mut self.stream, m)
    }

    pub fn recv<T: DeserializeOwned>(&mut self) -> Result<T> {
        read_frame(&mut self.stream)
    }

    pub fn set_timeout(&self, t: Option<Duration>) -> Result<()> {
        self.stream.tcp().set_read_timeout(t)?;
        Ok(())
    }
}

/// Another Mac as the DJ sees it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub name: String,
    /// Seen on the network now.
    pub online: bool,
    pub paired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Paired {
    id: String,
    name: String,
    /// Unix seconds.
    paired_at: u64,
}

struct Nearby {
    name: String,
    addrs: Vec<SocketAddr>,
    /// Its Bonjour name, to forget it when it leaves.
    fullname: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Macs came, left, paired or unpaired: ask [`Node::peers`] again.
    PeersChanged,
    /// Another Mac asks to pair; answer with [`Node::answer_pair`].
    PairRequest { request: u64, peer: Peer, code: String },
    /// The request was not answered in time.
    PairRequestGone { request: u64 },
    /// What came of [`Node::pair`].
    PairResult { peer: Peer, accepted: bool, message: String },
}

pub type OnEvent = Arc<dyn Fn(Event) + Send + Sync>;
pub type OnSession = Arc<dyn Fn(Session) + Send + Sync>;

pub struct Config {
    /// Where the identity and the paired Macs are kept.
    pub dir: PathBuf,
    /// This Mac's name as others see it.
    pub name: String,
    /// Announce and look for Macs with Bonjour (off in tests).
    pub discovery: bool,
    pub on_event: OnEvent,
    /// Serves a paired Mac that connected; by default answers pings.
    pub on_session: Option<OnSession>,
}

struct Inner {
    me: Identity,
    name: String,
    dir: PathBuf,
    tls: tls::Configs,
    paired: Mutex<Vec<Paired>>,
    nearby: Mutex<HashMap<String, Nearby>>,
    /// The pairing request waiting for the DJ's answer (one at a time).
    pending: Mutex<Option<(u64, mpsc::Sender<bool>)>>,
    next_request: AtomicU64,
    on_event: OnEvent,
    on_session: OnSession,
}

pub struct Node {
    inner: Arc<Inner>,
    port: u16,
    mdns: Option<ServiceDaemon>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn load_paired(dir: &Path) -> Vec<Paired> {
    std::fs::read(dir.join("paired.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Answer pings until the other side hangs up.
fn serve_pings(mut s: Session) {
    while let Ok(m) = s.recv::<Msg>() {
        if m == Msg::Ping && s.send(&Msg::Pong).is_err() {
            break;
        }
    }
}

impl Node {
    pub fn start(config: Config) -> Result<Node> {
        let me = Identity::load_or_create(&config.dir)?;
        let inner = Arc::new(Inner {
            tls: tls::configs(&me)?,
            me,
            name: config.name,
            paired: Mutex::new(load_paired(&config.dir)),
            dir: config.dir,
            nearby: Mutex::new(HashMap::new()),
            pending: Mutex::new(None),
            next_request: AtomicU64::new(1),
            on_event: config.on_event,
            on_session: config.on_session.unwrap_or_else(|| Arc::new(serve_pings)),
        });

        let listener = TcpListener::bind("0.0.0.0:0")?;
        let port = listener.local_addr()?.port();
        let server = inner.clone();
        std::thread::Builder::new().name("peer-listen".into()).spawn(move || {
            for sock in listener.incoming().flatten() {
                let inner = server.clone();
                let _ = std::thread::Builder::new()
                    .name("peer-in".into())
                    .spawn(move || {
                        let _ = inner.serve(sock);
                    });
            }
        })?;

        let mdns = if config.discovery { Some(discover(&inner, port)?) } else { None };
        Ok(Node { inner, port, mdns })
    }

    pub fn id(&self) -> &str {
        &self.inner.me.id
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Paired Macs (online or not) and unpaired ones on the network, by name.
    pub fn peers(&self) -> Vec<Peer> {
        let nearby = self.inner.nearby.lock().unwrap();
        let paired = self.inner.paired.lock().unwrap();
        let mut out: Vec<Peer> = paired
            .iter()
            .map(|p| Peer {
                id: p.id.clone(),
                name: nearby.get(&p.id).map_or(p.name.clone(), |n| n.name.clone()),
                online: nearby.contains_key(&p.id),
                paired: true,
            })
            .collect();
        for (id, n) in nearby.iter() {
            if !paired.iter().any(|p| p.id == *id) {
                out.push(Peer { id: id.clone(), name: n.name.clone(), online: true, paired: false });
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
        out
    }

    /// A Mac at a known address, as if Bonjour had found it (for networks
    /// that block it, and for tests).
    pub fn add_nearby(&self, id: &str, name: &str, addr: SocketAddr) {
        self.inner.nearby.lock().unwrap().insert(
            id.to_string(),
            Nearby { name: name.to_string(), addrs: vec![addr], fullname: String::new() },
        );
        self.inner.emit(Event::PeersChanged);
    }

    /// Ask a Mac on the network to pair. Returns the code to show; the
    /// answer comes as [`Event::PairResult`].
    pub fn pair(&self, peer_id: &str) -> Result<String> {
        let (mut stream, id) = self.inner.dial(peer_id)?;
        let code = pairing_code(&self.inner.me.id, &id);
        write_frame(&mut stream, &self.inner.hello(true))?;
        stream.tcp().set_read_timeout(Some(PAIR_TIMEOUT + Duration::from_secs(10)))?;
        let inner = self.inner.clone();
        std::thread::Builder::new().name("peer-pair".into()).spawn(move || {
            let name = inner.nearby.lock().unwrap().get(&id).map(|n| n.name.clone()).unwrap_or_default();
            let (accepted, message) = match read_frame::<Msg>(&mut stream) {
                Ok(Msg::PairAccepted) => {
                    inner.remember(&id, &name);
                    (true, format!("Spárováno s {name}."))
                }
                Ok(Msg::PairRejected) => (false, format!("{name} spárování odmítl.")),
                Ok(Msg::Busy) => (false, format!("{name} zrovna páruje s jiným Macem, zkus to za chvíli.")),
                Ok(Msg::Incompatible { .. }) => (false, format!("{name} má jinou verzi Stickwise, aktualizujte oba Macy.")),
                _ => (false, format!("Na {name} nikdo spárování nepotvrdil.")),
            };
            let peer = Peer { id, name, online: true, paired: accepted };
            inner.emit(Event::PairResult { peer, accepted, message });
            if accepted {
                inner.emit(Event::PeersChanged);
            }
        })?;
        Ok(code)
    }

    /// The DJ's answer to [`Event::PairRequest`].
    pub fn answer_pair(&self, request: u64, accept: bool) {
        if let Some((r, tx)) = self.inner.pending.lock().unwrap().as_ref()
            && *r == request
        {
            let _ = tx.send(accept);
        }
    }

    pub fn unpair(&self, peer_id: &str) -> Result<()> {
        self.inner.paired.lock().unwrap().retain(|p| p.id != peer_id);
        self.inner.save_paired()?;
        self.inner.emit(Event::PeersChanged);
        Ok(())
    }

    /// A connection to a paired Mac.
    pub fn connect(&self, peer_id: &str) -> Result<Session> {
        if !self.inner.is_paired(peer_id) {
            return refused("Tenhle Mac není spárovaný.");
        }
        let (mut stream, id) = self.inner.dial(peer_id)?;
        write_frame(&mut stream, &self.inner.hello(false))?;
        match read_frame::<Msg>(&mut stream)? {
            Msg::Hello { name, .. } => {
                self.inner.remember(&id, &name);
                Ok(Session { stream, peer_id: id, peer_name: name })
            }
            Msg::NotPaired => refused("Druhý Mac tenhle nezná (spárování zrušené?). Spárujte je znovu."),
            Msg::Incompatible { .. } => refused("Druhý Mac má jinou verzi Stickwise, aktualizujte oba."),
            _ => refused("Druhý Mac odpověděl nesrozumitelně."),
        }
    }

    /// Round trip to a paired Mac.
    pub fn ping(&self, peer_id: &str) -> Result<Duration> {
        let mut s = self.connect(peer_id)?;
        let t = Instant::now();
        s.send(&Msg::Ping)?;
        match s.recv::<Msg>()? {
            Msg::Pong => Ok(t.elapsed()),
            _ => refused("Druhý Mac odpověděl nesrozumitelně."),
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(m) = &self.mdns {
            let _ = m.shutdown();
        }
    }
}

impl Inner {
    fn emit(&self, e: Event) {
        (self.on_event)(e);
    }

    fn hello(&self, pair: bool) -> Msg {
        Msg::Hello { name: self.name.clone(), protocol: PROTOCOL, pair }
    }

    fn is_paired(&self, id: &str) -> bool {
        self.paired.lock().unwrap().iter().any(|p| p.id == id)
    }

    /// Pair with `id`, or keep its newest name if paired.
    fn remember(&self, id: &str, name: &str) {
        let mut paired = self.paired.lock().unwrap();
        match paired.iter_mut().find(|p| p.id == id) {
            Some(p) if p.name == name || name.is_empty() => return,
            Some(p) => p.name = name.to_string(),
            None => paired.push(Paired { id: id.to_string(), name: name.to_string(), paired_at: now_secs() }),
        }
        drop(paired);
        let _ = self.save_paired();
    }

    fn save_paired(&self) -> Result<()> {
        let json = serde_json::to_vec_pretty(&*self.paired.lock().unwrap())?;
        let tmp = self.dir.join("paired.json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, self.dir.join("paired.json"))?;
        Ok(())
    }

    /// TLS to a Mac on the network, checked to be the one asked for.
    fn dial(&self, peer_id: &str) -> Result<(tls::Stream, String)> {
        let addrs = match self.nearby.lock().unwrap().get(peer_id) {
            Some(n) => n.addrs.clone(),
            None => return refused("Ten Mac teď není ve stejné síti."),
        };
        let mut last = None;
        for a in addrs {
            match TcpStream::connect_timeout(&a, CONNECT_TIMEOUT) {
                Ok(sock) => {
                    sock.set_read_timeout(Some(IO_TIMEOUT))?;
                    sock.set_write_timeout(Some(IO_TIMEOUT))?;
                    sock.set_nodelay(true)?;
                    let (stream, id) = tls::Stream::connect(self.tls.client.clone(), sock)?;
                    if id != peer_id {
                        return refused("Na adrese toho Macu odpověděl jiný počítač.");
                    }
                    return Ok((stream, id));
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.map_or_else(|| Error::Refused("Ten Mac nemá žádnou adresu.".into()), Error::Io))
    }

    /// One incoming connection.
    fn serve(&self, sock: TcpStream) -> Result<()> {
        sock.set_read_timeout(Some(IO_TIMEOUT))?;
        sock.set_write_timeout(Some(IO_TIMEOUT))?;
        sock.set_nodelay(true)?;
        let (mut stream, id) = tls::Stream::accept(self.tls.server.clone(), sock)?;
        let Msg::Hello { name, protocol, pair } = read_frame(&mut stream)? else {
            return Ok(());
        };
        if protocol != PROTOCOL {
            return write_frame(&mut stream, &Msg::Incompatible { protocol: PROTOCOL });
        }
        if pair {
            let answer = self.ask_to_pair(&id, &name);
            if answer == Some(true) {
                self.remember(&id, &name);
                self.emit(Event::PeersChanged);
            }
            let reply = match answer {
                None => Msg::Busy,
                Some(true) => Msg::PairAccepted,
                Some(false) => Msg::PairRejected,
            };
            return write_frame(&mut stream, &reply);
        }
        if !self.is_paired(&id) {
            return write_frame(&mut stream, &Msg::NotPaired);
        }
        self.remember(&id, &name);
        write_frame(&mut stream, &self.hello(false))?;
        (self.on_session)(Session { stream, peer_id: id, peer_name: name });
        Ok(())
    }

    /// Show the request to the DJ and wait for the answer: `None` while
    /// another request is waiting, `Some(false)` when not answered in time.
    fn ask_to_pair(&self, id: &str, name: &str) -> Option<bool> {
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            if pending.is_some() {
                return None;
            }
            *pending = Some((request, tx));
        }
        let peer = Peer { id: id.to_string(), name: name.to_string(), online: true, paired: self.is_paired(id) };
        let code = pairing_code(&self.me.id, id);
        self.emit(Event::PairRequest { request, peer, code });
        let answer = rx.recv_timeout(PAIR_TIMEOUT).ok();
        *self.pending.lock().unwrap() = None;
        if answer.is_none() {
            self.emit(Event::PairRequestGone { request });
        }
        Some(answer.unwrap_or(false))
    }
}

/// Announce this Mac and keep `nearby` up to date with the others.
fn discover(inner: &Arc<Inner>, port: u16) -> Result<ServiceDaemon> {
    let mdns = ServiceDaemon::new()?;
    let short = &inner.me.id[..12];
    let props = [("id", inner.me.id.as_str()), ("name", inner.name.as_str()), ("v", "1")];
    let info = ServiceInfo::new(
        SERVICE,
        &format!("Stickwise {short}"),
        &format!("stickwise-{short}.local."),
        "",
        port,
        &props[..],
    )?
    .enable_addr_auto();
    mdns.register(info)?;
    let events = mdns.browse(SERVICE)?;
    let inner = inner.clone();
    std::thread::Builder::new().name("peer-browse".into()).spawn(move || {
        while let Ok(ev) = events.recv() {
            match ev {
                ServiceEvent::ServiceResolved(s) => {
                    let Some(id) = s.get_property_val_str("id").map(str::to_string) else { continue };
                    if id == inner.me.id {
                        continue;
                    }
                    let mut addrs: Vec<SocketAddr> =
                        s.get_addresses_v4().into_iter().map(|ip| SocketAddr::new(ip.into(), s.get_port())).collect();
                    if addrs.is_empty() {
                        continue;
                    }
                    addrs.sort();
                    let name = s.get_property_val_str("name").filter(|n| !n.is_empty()).unwrap_or("Mac").to_string();
                    let fullname = s.get_fullname().to_string();
                    inner.nearby.lock().unwrap().insert(id, Nearby { name, addrs, fullname });
                    inner.emit(Event::PeersChanged);
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    inner.nearby.lock().unwrap().retain(|_, n| n.fullname != fullname);
                    inner.emit(Event::PeersChanged);
                }
                _ => {}
            }
        }
    })?;
    Ok(mdns)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Test {
        node: Node,
        events: mpsc::Receiver<Event>,
        _dir: tempfile::TempDir,
    }

    fn node(name: &str) -> Test {
        let dir = tempfile::tempdir().unwrap();
        let (tx, events) = mpsc::channel();
        let tx = Mutex::new(tx);
        let node = Node::start(Config {
            dir: dir.path().to_path_buf(),
            name: name.into(),
            discovery: false,
            on_event: Arc::new(move |e| {
                let _ = tx.lock().unwrap().send(e);
            }),
            on_session: None,
        })
        .unwrap();
        Test { node, events, _dir: dir }
    }

    /// Each knows where the other is, as Bonjour would tell them.
    fn neighbours(a: &Test, b: &Test) {
        a.node.add_nearby(b.node.id(), "B", ([127, 0, 0, 1], b.node.port()).into());
        b.node.add_nearby(a.node.id(), "A", ([127, 0, 0, 1], a.node.port()).into());
    }

    fn next(events: &mpsc::Receiver<Event>, want: impl Fn(&Event) -> bool) -> Event {
        loop {
            let e = events.recv_timeout(Duration::from_secs(10)).expect("no event");
            if want(&e) {
                return e;
            }
        }
    }

    #[test]
    fn pairs_after_the_other_side_accepts_then_connects() {
        let (a, b) = (node("A"), node("B"));
        neighbours(&a, &b);
        // Not paired yet: no connection.
        assert!(a.node.ping(b.node.id()).is_err());

        let code = a.node.pair(b.node.id()).unwrap();
        let Event::PairRequest { request, peer, code: shown } =
            next(&b.events, |e| matches!(e, Event::PairRequest { .. }))
        else {
            unreachable!()
        };
        assert_eq!(shown, code);
        assert_eq!(peer.name, "A");
        assert_eq!(peer.id, a.node.id());
        b.node.answer_pair(request, true);
        let Event::PairResult { accepted, peer, .. } = next(&a.events, |e| matches!(e, Event::PairResult { .. })) else {
            unreachable!()
        };
        assert!(accepted && peer.paired);

        assert!(a.node.peers()[0].paired && b.node.peers()[0].paired);
        assert!(a.node.ping(b.node.id()).is_ok());
        assert!(b.node.ping(a.node.id()).is_ok());
    }

    #[test]
    fn rejected_pairing_leaves_both_unpaired() {
        let (a, b) = (node("A"), node("B"));
        neighbours(&a, &b);
        a.node.pair(b.node.id()).unwrap();
        let Event::PairRequest { request, .. } = next(&b.events, |e| matches!(e, Event::PairRequest { .. })) else {
            unreachable!()
        };
        b.node.answer_pair(request, false);
        let Event::PairResult { accepted, .. } = next(&a.events, |e| matches!(e, Event::PairResult { .. })) else {
            unreachable!()
        };
        assert!(!accepted);
        assert!(!a.node.peers()[0].paired && !b.node.peers()[0].paired);
        assert!(a.node.connect(b.node.id()).is_err());
    }

    #[test]
    fn unpairing_on_one_side_stops_connections() {
        let (a, b) = (node("A"), node("B"));
        neighbours(&a, &b);
        a.node.pair(b.node.id()).unwrap();
        let Event::PairRequest { request, .. } = next(&b.events, |e| matches!(e, Event::PairRequest { .. })) else {
            unreachable!()
        };
        b.node.answer_pair(request, true);
        next(&a.events, |e| matches!(e, Event::PairResult { .. }));

        b.node.unpair(a.node.id()).unwrap();
        let e = a.node.connect(b.node.id()).err().unwrap().to_string();
        assert!(e.contains("Spárujte je znovu"), "{e}");
    }

    #[test]
    fn an_impostor_at_the_address_is_refused() {
        let (a, b, c) = (node("A"), node("B"), node("C"));
        // A thinks B lives where C is.
        a.node.add_nearby(b.node.id(), "B", ([127, 0, 0, 1], c.node.port()).into());
        let e = a.node.pair(b.node.id()).err().unwrap().to_string();
        assert!(e.contains("jiný počítač"), "{e}");
    }

    /// Real Bonjour on this machine's network (needs multicast, so not in CI).
    #[test]
    #[ignore]
    fn finds_each_other_with_bonjour() {
        let start = |name: &str, dir: &Path| {
            let (tx, rx) = mpsc::channel();
            let tx = Mutex::new(tx);
            let node = Node::start(Config {
                dir: dir.to_path_buf(),
                name: name.into(),
                discovery: true,
                on_event: Arc::new(move |e| {
                    let _ = tx.lock().unwrap().send(e);
                }),
                on_session: None,
            })
            .unwrap();
            (node, rx)
        };
        let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, _ea) = start("Tomášův MacBook", da.path());
        let (b, _eb) = start("Studio", db.path());
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(15) {
            let (pa, pb) = (a.peers(), b.peers());
            if pa.iter().any(|p| p.id == b.id()) && pb.iter().any(|p| p.id == a.id()) {
                assert_eq!(pb.iter().find(|p| p.id == a.id()).unwrap().name, "Tomášův MacBook");
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("not found: a sees {:?}, b sees {:?}", a.peers(), b.peers());
    }

    #[test]
    fn paired_macs_are_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let start = || {
            Node::start(Config {
                dir: dir.path().to_path_buf(),
                name: "A".into(),
                discovery: false,
                on_event: Arc::new(|_| {}),
                on_session: None,
            })
            .unwrap()
        };
        let a = start();
        a.inner.remember("b1", "B");
        let id = a.id().to_string();
        drop(a);
        let a = start();
        assert_eq!(a.id(), id);
        assert_eq!(a.peers(), vec![Peer { id: "b1".into(), name: "B".into(), online: false, paired: true }]);
    }
}
