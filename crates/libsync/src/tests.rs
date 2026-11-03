use std::sync::{Arc, mpsc};

use peerlink::{Config, Event, Node};
use pioneer_formats::anlz::CuePoint;

use super::*;

struct TestHost {
    lib: Mutex<Library>,
    music: PathBuf,
    anlz: PathBuf,
    accept: bool,
    asked: Mutex<Vec<Summary>>,
    done: mpsc::Sender<std::result::Result<ApplyReport, String>>,
}

impl Host for TestHost {
    fn library(&self) -> &Mutex<Library> {
        &self.lib
    }
    fn music_dir(&self) -> PathBuf {
        self.music.clone()
    }
    fn anlz_dir(&self) -> PathBuf {
        self.anlz.clone()
    }
    fn ask(&self, _: &str, s: &Summary) -> bool {
        self.asked.lock().unwrap().push(s.clone());
        self.accept
    }
    fn progress(&self, _: &str, _: &Progress) -> bool {
        true
    }
    fn finished(&self, _: &str, r: &std::result::Result<ApplyReport, String>) {
        let _ = self.done.send(r.clone());
    }
}

struct World {
    a: Node,
    a_lib: Mutex<Library>,
    a_music: PathBuf,
    b: Node,
    host: Arc<TestHost>,
    done: mpsc::Receiver<std::result::Result<ApplyReport, String>>,
    _dir: tempfile::TempDir,
}

fn node(dir: &Path, name: &str, events: mpsc::Sender<Event>, on_session: Option<peerlink::OnSession>) -> Node {
    let events = Mutex::new(events);
    Node::start(Config {
        dir: dir.to_path_buf(),
        name: name.into(),
        discovery: false,
        on_event: Arc::new(move |e| {
            let _ = events.lock().unwrap().send(e);
        }),
        on_session,
    })
    .unwrap()
}

fn wait(rx: &mpsc::Receiver<Event>, want: impl Fn(&Event) -> bool) -> Event {
    loop {
        let e = rx.recv_timeout(Duration::from_secs(10)).expect("no event");
        if want(&e) {
            return e;
        }
    }
}

/// Mac A with a playlist "Sobota" of two tracks (one with cues and an
/// analysis), Mac B empty, paired; B already has a different file where
/// A's first track would go.
fn world(accept: bool) -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let a_music = root.join("a-music");
    let set = a_music.join("Sobota");
    std::fs::create_dir_all(&set).unwrap();
    std::fs::write(set.join("DJ A - Jedna.mp3"), vec![1u8; 700_000]).unwrap();
    std::fs::write(set.join("DJ B - Dva.mp3"), b"second track").unwrap();

    let mut a_lib = Library::in_memory().unwrap();
    a_lib.add_scan(&library::folders::scan(std::slice::from_ref(&set)), 0).unwrap();
    for s in a_lib.file_stamps().unwrap() {
        let fp = library::peer::fingerprint_file(Path::new(&s.path)).unwrap();
        let size = std::fs::metadata(&s.path).unwrap().len();
        a_lib.set_fingerprint(s.id, &fp, size, 1).unwrap();
    }
    let one = a_lib.tracks().unwrap().into_iter().find(|t| t.title == "Jedna").unwrap().id;
    a_lib.set_cues(one, &[CuePoint { comment: "Drop".into(), ..CuePoint::hot(1, 64_000) }]).unwrap();
    let anlz = root.join("a-anlz");
    std::fs::create_dir_all(&anlz).unwrap();
    std::fs::write(anlz.join("ANLZ0000.DAT"), b"dat").unwrap();
    std::fs::write(anlz.join("ANLZ0000.EXT"), b"ext").unwrap();
    a_lib.set_analysis(one, &anlz.join("ANLZ0000.DAT").display().to_string(), None, None, 0.0).unwrap();

    let b_music = root.join("b-music");
    let taken = b_music.join("DJ A").join("Neznámé album");
    std::fs::create_dir_all(&taken).unwrap();
    std::fs::write(taken.join("DJ A - Jedna.mp3"), b"the DJ's own, other file").unwrap();

    let (done_tx, done) = mpsc::channel();
    let host = Arc::new(TestHost {
        lib: Mutex::new(Library::in_memory().unwrap()),
        music: b_music,
        anlz: root.join("b-anlz"),
        accept,
        asked: Mutex::new(Vec::new()),
        done: done_tx,
    });
    let serving = host.clone();
    let (ae_tx, ae) = mpsc::channel();
    let (be_tx, be) = mpsc::channel();
    let a = node(&root.join("a-peer"), "A", ae_tx, None);
    let b = node(&root.join("b-peer"), "B", be_tx, Some(Arc::new(move |s| serve(s, serving.as_ref()))));
    a.add_nearby(b.id(), "B", ([127, 0, 0, 1], b.port()).into());
    b.add_nearby(a.id(), "A", ([127, 0, 0, 1], a.port()).into());
    a.pair(b.id()).unwrap();
    let Event::PairRequest { request, .. } = wait(&be, |e| matches!(e, Event::PairRequest { .. })) else { unreachable!() };
    b.answer_pair(request, true);
    wait(&ae, |e| matches!(e, Event::PairResult { .. }));

    World { a, a_lib: Mutex::new(a_lib), a_music, b, host, done, _dir: dir }
}

fn send_all(w: &World) -> Outcome {
    let offer = {
        let l = w.a_lib.lock().unwrap();
        let p = l.playlists().unwrap()[0].id;
        l.offer(&[p], &[]).unwrap()
    };
    let mut s = w.a.connect(w.b.id()).unwrap();
    send(&mut s, &w.a_lib, offer, |_| true).unwrap()
}

#[test]
fn sends_a_playlist_with_files_cues_and_analysis() {
    let w = world(true);
    let Outcome::Sent(r) = send_all(&w) else { panic!("declined") };
    assert_eq!((r.tracks_added, r.playlists_added), (2, 1));
    assert!(r.skipped.is_empty(), "{:?}", r.skipped);
    assert_eq!(w.done.recv_timeout(Duration::from_secs(5)).unwrap(), Ok(r));
    let asked = w.host.asked.lock().unwrap()[0].clone();
    assert_eq!((asked.tracks, asked.new_tracks, asked.new_bytes), (2, 2, 700_012));
    assert_eq!(asked.playlists, vec!["Sobota"]);

    let b = w.host.lib.lock().unwrap();
    let tracks = b.tracks().unwrap();
    let one = tracks.iter().find(|t| t.title == "Jedna").unwrap();
    // The file already there is left alone; the new one sits beside it.
    let dir = w.host.music.join("DJ A").join("Neznámé album");
    assert_eq!(std::fs::read(dir.join("DJ A - Jedna.mp3")).unwrap(), b"the DJ's own, other file");
    assert_eq!(Path::new(&one.path), dir.join("DJ A - Jedna (2).mp3"));
    assert_eq!(std::fs::read(&one.path).unwrap(), vec![1u8; 700_000]);
    assert_eq!(b.cues(one.id).unwrap()[0].comment, "Drop");
    assert_eq!(std::fs::read(Path::new(&one.anlz).with_extension("EXT")).unwrap(), b"ext");
    assert!(one.anlz.starts_with(w.host.anlz.to_str().unwrap()));
    let two = tracks.iter().find(|t| t.title == "Dva").unwrap();
    assert_eq!(std::fs::read(&two.path).unwrap(), b"second track");
    assert!(two.anlz.is_empty());
    let p = &b.playlists().unwrap()[0];
    assert_eq!(p.name, "Sobota");
    assert_eq!(p.track_ids.len(), 2);
    // Received files count as fingerprinted.
    assert_eq!(b.fingerprint_progress().unwrap(), (2, 2));
    // Nothing of A's was touched.
    assert!(w.a_music.join("Sobota/DJ A - Jedna.mp3").is_file());
}

#[test]
fn sending_again_moves_no_files() {
    let w = world(true);
    send_all(&w);
    let Outcome::Sent(r) = send_all(&w) else { panic!("declined") };
    assert_eq!(r, ApplyReport::default());
    let asked = w.host.asked.lock().unwrap()[1].clone();
    assert_eq!(asked.new_tracks, 0);
    assert_eq!(w.host.lib.lock().unwrap().tracks().unwrap().len(), 2);
}

#[test]
fn a_declined_offer_changes_nothing() {
    let w = world(false);
    assert!(matches!(send_all(&w), Outcome::Declined(_)));
    assert!(w.host.lib.lock().unwrap().tracks().unwrap().is_empty());
    assert!(!w.host.music.join("DJ B").exists());
}

#[test]
fn stopping_midway_keeps_no_part_files() {
    let w = world(true);
    let offer = {
        let l = w.a_lib.lock().unwrap();
        l.offer(&[l.playlists().unwrap()[0].id], &[]).unwrap()
    };
    let mut s = w.a.connect(w.b.id()).unwrap();
    let mut calls = 0;
    let r = send(&mut s, &w.a_lib, offer, |p| {
        calls += 1;
        p.bytes_done < 300_000
    });
    assert!(r.is_err());
    drop(s);
    assert!(w.done.recv_timeout(Duration::from_secs(5)).unwrap().is_err());
    assert!(w.host.lib.lock().unwrap().tracks().unwrap().is_empty());
    let left: Vec<_> = walk(&w.host.music).into_iter().filter(|p| p.to_string_lossy().contains("stickwise-part")).collect();
    assert!(left.is_empty(), "{left:?}");
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn pings_still_answered() {
    let w = world(true);
    assert!(w.a.ping(w.b.id()).is_ok());
}

#[test]
fn names_are_made_safe() {
    assert_eq!(component("AC/DC: Live", "x"), "AC-DC- Live");
    assert_eq!(component("  ..hidden ", "x"), "hidden");
    assert_eq!(component("", "Neznámý interpret"), "Neznámý interpret");
}
