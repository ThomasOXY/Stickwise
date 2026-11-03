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
    fn started(&self, _: &str, _: bool) {}
    fn progress(&self, _: &str, _: &Progress) -> bool {
        true
    }
    fn finished(&self, _: &str, r: &std::result::Result<ApplyReport, String>) {
        let _ = self.done.send(r.clone());
    }
    fn cancelled(&self, _: &str) {
        let _ = self.done.send(Err("cancelled".into()));
    }
}

struct World {
    a: Node,
    b: Node,
    /// A's side (it starts sends and mirrors).
    ah: Arc<TestHost>,
    /// B's side (serving).
    host: Arc<TestHost>,
    a_src: PathBuf,
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

/// A library holding the audio files of folder `src` as a playlist.
fn library_of(src: &Path) -> Library {
    let mut lib = Library::in_memory().unwrap();
    lib.add_scan(&library::folders::scan(&[src.to_path_buf()]), 0).unwrap();
    lib
}

fn host(lib: Library, root: &Path, side: &str, accept: bool) -> (Arc<TestHost>, mpsc::Receiver<std::result::Result<ApplyReport, String>>) {
    let (tx, rx) = mpsc::channel();
    let h = Arc::new(TestHost {
        lib: Mutex::new(lib),
        music: root.join(format!("{side}-music")),
        anlz: root.join(format!("{side}-anlz")),
        accept,
        asked: Mutex::new(Vec::new()),
        done: tx,
    });
    (h, rx)
}

/// Mac A with a playlist "Sobota" of two tracks (one with cues and an
/// analysis), Mac B with a playlist "Doma" of one track, paired; B already
/// has a different file where A's first track would go.
fn world(accept: bool) -> World {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let a_src = root.join("a-src/Sobota");
    std::fs::create_dir_all(&a_src).unwrap();
    std::fs::write(a_src.join("DJ A - Jedna.mp3"), vec![1u8; 700_000]).unwrap();
    std::fs::write(a_src.join("DJ B - Dva.mp3"), b"second track").unwrap();
    let b_src = root.join("b-src/Doma");
    std::fs::create_dir_all(&b_src).unwrap();
    std::fs::write(b_src.join("DJ C - Tři.mp3"), b"b's own track").unwrap();

    let mut a_lib = library_of(&a_src);
    let one = a_lib.tracks().unwrap().into_iter().find(|t| t.title == "Jedna").unwrap().id;
    a_lib.set_cues(one, &[CuePoint { comment: "Drop".into(), ..CuePoint::hot(1, 64_000) }]).unwrap();
    let anlz = root.join("a-anlz-own");
    std::fs::create_dir_all(&anlz).unwrap();
    std::fs::write(anlz.join("ANLZ0000.DAT"), b"dat").unwrap();
    std::fs::write(anlz.join("ANLZ0000.EXT"), b"ext").unwrap();
    a_lib.set_analysis(one, &anlz.join("ANLZ0000.DAT").display().to_string(), None, None, 0.0).unwrap();

    let (ah, _) = host(a_lib, root, "a", true);
    let (bh, done) = host(library_of(&b_src), root, "b", accept);
    let taken = bh.music.join("DJ A").join("Neznámé album");
    std::fs::create_dir_all(&taken).unwrap();
    std::fs::write(taken.join("DJ A - Jedna.mp3"), b"the DJ's own, other file").unwrap();
    for h in [&ah, &bh] {
        fingerprint(&h.lib, None, |_, _| true).unwrap();
    }

    let serving = bh.clone();
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

    World { a, b, ah, host: bh, a_src, done, _dir: dir }
}

fn send_all(w: &World) -> Outcome {
    let offer = {
        let l = w.ah.lib.lock().unwrap();
        let p = l.playlists().unwrap()[0].id;
        l.offer(&[p], &[]).unwrap()
    };
    let mut s = w.a.connect(w.b.id()).unwrap();
    send(&mut s, &w.ah.lib, offer, |_| true).unwrap()
}

/// Mirror from A; `choose` sees the preview and gives the go.
fn mirror_with(w: &World, choose: impl FnOnce(&MirrorPreview) -> Option<MirrorChoice>) -> (MirrorOutcome, MirrorPreview) {
    let mut s = w.a.connect(w.b.id()).unwrap();
    let mut seen = None;
    let out = mirror(&mut s, w.ah.as_ref(), |p| {
        seen = Some(p.clone());
        choose(p)
    })
    .unwrap();
    (out, seen.unwrap())
}

fn names(lib: &Mutex<Library>) -> Vec<String> {
    let mut n: Vec<String> = lib.lock().unwrap().playlists().unwrap().into_iter().map(|p| p.name).collect();
    n.sort();
    n
}

fn titles(lib: &Mutex<Library>) -> Vec<String> {
    let mut t: Vec<String> = lib.lock().unwrap().tracks().unwrap().into_iter().map(|t| t.title).collect();
    t.sort();
    t
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
    let p = b.playlists().unwrap().into_iter().find(|p| p.name == "Sobota").unwrap();
    assert_eq!(p.track_ids.len(), 2);
    // Received files count as fingerprinted.
    assert_eq!(b.fingerprint_progress().unwrap(), (3, 3));
    // Nothing of A's was touched.
    assert!(w.a_src.join("DJ A - Jedna.mp3").is_file());
}

#[test]
fn sending_again_moves_no_files() {
    let w = world(true);
    send_all(&w);
    let Outcome::Sent(r) = send_all(&w) else { panic!("declined") };
    assert_eq!(r, ApplyReport::default());
    let asked = w.host.asked.lock().unwrap()[1].clone();
    assert_eq!(asked.new_tracks, 0);
    assert_eq!(w.host.lib.lock().unwrap().tracks().unwrap().len(), 3);
}

#[test]
fn a_declined_offer_changes_nothing() {
    let w = world(false);
    assert!(matches!(send_all(&w), Outcome::Declined(_)));
    assert_eq!(titles(&w.host.lib), ["Tři"]);
    assert!(!w.host.music.join("DJ B").exists());
}

#[test]
fn stopping_midway_keeps_no_part_files() {
    let w = world(true);
    let offer = {
        let l = w.ah.lib.lock().unwrap();
        l.offer(&[l.playlists().unwrap()[0].id], &[]).unwrap()
    };
    let mut s = w.a.connect(w.b.id()).unwrap();
    let r = send(&mut s, &w.ah.lib, offer, |p| p.bytes_done < 300_000);
    assert!(r.is_err());
    drop(s);
    assert!(w.done.recv_timeout(Duration::from_secs(5)).unwrap().is_err());
    assert_eq!(titles(&w.host.lib), ["Tři"]);
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
fn mirror_makes_both_libraries_the_same() {
    let w = world(true);
    let (out, preview) = mirror_with(&w, |_| Some(MirrorChoice::default()));
    assert_eq!((preview.here.report.tracks_added, preview.here.new_bytes), (1, 13));
    assert_eq!((preview.there.report.tracks_added, preview.there.new_bytes), (2, 700_012));
    assert_eq!(preview.there.report.playlists_added, 1);
    assert!(preview.delete_here.is_empty() && preview.delete_there.is_empty());
    let MirrorOutcome::Done { here, there } = out else { panic!("cancelled") };
    assert_eq!((here.tracks_added, there.tracks_added), (1, 2));
    assert!(w.done.recv_timeout(Duration::from_secs(5)).unwrap().is_ok());

    for h in [&w.ah, &w.host] {
        assert_eq!(names(&h.lib), ["Doma", "Sobota"]);
        assert_eq!(titles(&h.lib), ["Dva", "Jedna", "Tři"]);
    }
    {
        let a = w.ah.lib.lock().unwrap();
        let tri = a.tracks().unwrap().into_iter().find(|t| t.title == "Tři").unwrap();
        assert!(tri.path.starts_with(w.ah.music.to_str().unwrap()));
        assert_eq!(std::fs::read(&tri.path).unwrap(), b"b's own track");
    }

    // Nothing changed since: a second mirror moves nothing.
    let (out, preview) = mirror_with(&w, |_| Some(MirrorChoice::default()));
    assert_eq!(preview.here, Changes::default());
    assert_eq!(preview.there, Changes::default());
    assert_eq!(out, MirrorOutcome::Done { here: ApplyReport::default(), there: ApplyReport::default() });
}

#[test]
fn mirror_carries_newer_cues_from_the_other_side() {
    let w = world(true);
    mirror_with(&w, |_| Some(MirrorChoice::default()));
    std::thread::sleep(Duration::from_millis(5));
    {
        let mut b = w.host.lib.lock().unwrap();
        let id = b.tracks().unwrap().into_iter().find(|t| t.title == "Jedna").unwrap().id;
        b.set_cues(id, &[CuePoint { comment: "Break".into(), ..CuePoint::hot(2, 90_000) }]).unwrap();
    }
    let (_, preview) = mirror_with(&w, |_| Some(MirrorChoice::default()));
    assert_eq!(preview.here.report.cues_updated, 1);
    let a = w.ah.lib.lock().unwrap();
    let id = a.tracks().unwrap().into_iter().find(|t| t.title == "Jedna").unwrap().id;
    assert_eq!(a.cues(id).unwrap()[0].comment, "Break");
    assert!(a.has_cue_backup(id).unwrap(), "A's own cues are kept to restore");
}

#[test]
fn a_confirmed_deletion_goes_across_and_an_unconfirmed_one_comes_back() {
    let w = world(true);
    mirror_with(&w, |_| Some(MirrorChoice::default()));
    // B deletes "Doma", A deletes "Sobota".
    for (h, name) in [(&w.host, "Doma"), (&w.ah, "Sobota")] {
        let l = h.lib.lock().unwrap();
        let id = l.playlists().unwrap().into_iter().find(|p| p.name == name).unwrap().id;
        l.delete_playlist(id).unwrap();
    }
    // The DJ confirms deleting "Doma" here too, but keeps "Sobota" on B.
    let (out, preview) = mirror_with(&w, |p| {
        Some(MirrorChoice { delete_here: p.delete_here.iter().map(|d| d.uid.clone()).collect(), delete_there: Vec::new() })
    });
    assert_eq!(preview.delete_here.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Doma"]);
    assert_eq!(preview.delete_there.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Sobota"]);
    let MirrorOutcome::Done { here, .. } = out else { panic!("cancelled") };
    assert_eq!(here.playlists_deleted, 1);
    for h in [&w.ah, &w.host] {
        assert_eq!(names(&h.lib), ["Sobota"]);
        // Tracks stay whatever happens to playlists.
        assert_eq!(titles(&h.lib).len(), 3);
    }
}

#[test]
fn a_cancelled_mirror_changes_nothing() {
    let w = world(true);
    let (out, _) = mirror_with(&w, |_| None);
    assert_eq!(out, MirrorOutcome::Cancelled);
    assert_eq!(w.done.recv_timeout(Duration::from_secs(5)).unwrap(), Err("cancelled".into()));
    assert_eq!(names(&w.ah.lib), ["Sobota"]);
    assert_eq!(names(&w.host.lib), ["Doma"]);
}

#[test]
fn pings_still_answered() {
    let w = world(true);
    assert!(w.a.ping(w.b.id()).is_ok());
}

#[test]
fn failures_explained_once_even_with_brackets_in_the_title() {
    let mut r = ApplyReport { skipped: vec!["DJ – Track (Original Mix)".into()], ..Default::default() };
    explain(&mut r, vec![("DJ – Track (Original Mix)".into(), "soubor chybí".into())]);
    assert_eq!(r.skipped, ["DJ – Track (Original Mix) (soubor chybí)"]);
}

#[test]
fn names_are_made_safe() {
    assert_eq!(component("AC/DC: Live", "x"), "AC-DC- Live");
    assert_eq!(component("  ..hidden ", "x"), "hidden");
    assert_eq!(component("", "Neznámý interpret"), "Neznámý interpret");
}
