use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pioneer_formats::anlz::{AnlzFile, CueType, Waveform, WaveformKind};
use pioneer_formats::pdb::{Database, table};
use usb_tools::Stick;
use usb_tools::extract;
use usb_tools::{compat, verify};

#[derive(Parser)]
#[command(name = "rbx", version, about = "RB-EXPORT: Pioneer USB tools")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show the playlists and tracks of a USB stick (its root or export.pdb).
    Usb { path: PathBuf },
    /// Dump tables and pages of an export.pdb.
    Pdb { file: PathBuf },
    /// Dump an ANLZ0000.DAT / .EXT / .2EX file.
    Anlz { file: PathBuf },
    /// Health check of a stick: files, analysis, formats per player, filesystem.
    Verify {
        usb: PathBuf,
        /// Only check these players (default: all).
        #[arg(long, value_enum)]
        player: Vec<PlayerArg>,
    },
    /// Smart Cue: suggest structure cues; with --eval compare with existing hot cues.
    SmartCue {
        usb: PathBuf,
        /// Only this track id.
        #[arg(long)]
        track: Option<u32>,
        /// Score suggestions against the hot cues already on the stick.
        #[arg(long)]
        eval: bool,
        /// Only print the audio paths of tracks that have hot cues.
        #[arg(long)]
        list_cued: bool,
        /// Also score CUE-DETR results (JSON lines from sidecar/cue_detr.py).
        #[arg(long)]
        detr: Option<PathBuf>,
        /// CUE-DETR score threshold.
        #[arg(long, default_value_t = 0.9)]
        detr_threshold: f32,
    },
    /// Copy playlists from a stick into folders: <dest>/<folder>/<playlist>/files.
    Extract {
        /// Root of the mounted stick, e.g. /Volumes/DJ.
        usb: PathBuf,
        /// Destination folder.
        dest: PathBuf,
        /// Playlist name to extract (repeatable); default all.
        #[arg(short, long = "playlist")]
        playlists: Vec<String>,
        /// Prefix files with their position in the playlist ("01 - ").
        #[arg(short, long)]
        numbered: bool,
        /// Name files "Artist - Title" instead of keeping the stick's file name.
        #[arg(long)]
        artist_title: bool,
        /// How to store a track that is in several playlists.
        #[arg(long, value_enum, default_value = "clone")]
        dupes: Dupes,
        /// Also extract the players' HISTORY playlists.
        #[arg(long)]
        history: bool,
        /// Do not write .m3u8 files.
        #[arg(long)]
        no_m3u: bool,
        /// Only show what would be copied.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PlayerArg {
    Nxs,
    Nxs2,
    Cdj3000,
    Cdj3000x,
}

impl From<PlayerArg> for compat::Player {
    fn from(p: PlayerArg) -> Self {
        match p {
            PlayerArg::Nxs => compat::Player::Cdj2000Nxs,
            PlayerArg::Nxs2 => compat::Player::Cdj2000Nxs2,
            PlayerArg::Cdj3000 => compat::Player::Cdj3000,
            PlayerArg::Cdj3000x => compat::Player::Cdj3000X,
        }
    }
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Dupes {
    /// APFS clone: no extra space.
    Clone,
    Copy,
    Symlink,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Usb { path } => usb(&path),
        Cmd::Pdb { file } => pdb(&file),
        Cmd::Anlz { file } => anlz(&file),
        Cmd::Verify { usb, player } => verify_cmd(&usb, &player),
        Cmd::SmartCue {
            usb,
            track,
            eval,
            list_cued,
            detr,
            detr_threshold,
        } => smart_cue_cmd(
            &usb,
            track,
            eval,
            list_cued,
            detr.as_deref(),
            detr_threshold,
        ),
        Cmd::Extract {
            usb,
            dest,
            playlists,
            numbered,
            artist_title,
            dupes,
            history,
            no_m3u,
            dry_run,
        } => {
            let opts = extract::ExtractOptions {
                playlists: Vec::new(),
                history,
                numbered,
                naming: if artist_title {
                    extract::Naming::ArtistTitle
                } else {
                    extract::Naming::Original
                },
                duplicates: match dupes {
                    Dupes::Clone => extract::Duplicates::Clone,
                    Dupes::Copy => extract::Duplicates::Copy,
                    Dupes::Symlink => extract::Duplicates::Symlink,
                },
                m3u: !no_m3u,
            };
            extract_cmd(&usb, &dest, &playlists, opts, dry_run)
        }
    }
}

fn pdb_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("PIONEER/rekordbox/export.pdb")
    } else {
        path.to_path_buf()
    }
}

fn load_db(path: &Path) -> Result<Database> {
    let file = pdb_path(path);
    let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
    Database::parse(&bytes).with_context(|| format!("parsing {}", file.display()))
}

fn print_problems(db: &Database) {
    if db.problems.is_empty() {
        return;
    }
    println!("\n⚠ {} problem(s):", db.problems.len());
    for p in &db.problems {
        println!("  {p}");
    }
}

fn usb(path: &Path) -> Result<()> {
    let db = load_db(path)?;
    println!(
        "{} tracks, {} playlists/folders, {} history playlists",
        db.tracks.len(),
        db.playlists.len(),
        db.history_playlists.len()
    );
    fn walk(db: &Database, parent: u32, depth: usize) {
        for node in db.playlist_children(parent) {
            let indent = "  ".repeat(depth);
            if node.is_folder {
                println!("{indent}📁 {}", node.name);
                walk(db, node.id, depth + 1);
            } else {
                let ids = db.playlist_tracks(node.id);
                println!("{indent}🎵 {} ({} tracks)", node.name, ids.len());
                for (i, id) in ids.iter().enumerate() {
                    match db.track(*id) {
                        Some(t) => println!(
                            "{indent}   {:>3}. {} – {}  [{:.2} BPM, {}]",
                            i + 1,
                            db.artist_name(t.artist_id).unwrap_or("?"),
                            t.title,
                            t.bpm(),
                            db.key_name(t.key_id).unwrap_or("-")
                        ),
                        None => println!("{indent}   {:>3}. <missing track {id}>", i + 1),
                    }
                }
            }
        }
    }
    walk(&db, 0, 0);
    print_problems(&db);
    Ok(())
}

fn pdb(file: &Path) -> Result<()> {
    let db = load_db(file)?;
    let h = db.header.as_ref().unwrap();
    println!(
        "page_len={} next_unused={} sequence={} tables={}",
        h.page_len,
        h.next_unused_page,
        h.sequence,
        h.tables.len()
    );
    for t in &h.tables {
        println!(
            "  {:>2} {:<18} first={:<4} last={:<4} empty={}",
            t.ty,
            table::name(t.ty),
            t.first_page,
            t.last_page,
            t.empty_candidate
        );
    }
    println!("\ntracks:");
    for t in &db.tracks {
        println!(
            "  #{:<4} {:<40} {:>7.2} BPM  {}s  {}  anlz={}",
            t.id,
            t.title,
            t.bpm(),
            t.duration,
            t.file_path,
            t.analyze_path
        );
    }
    println!(
        "\nartists: {:?}",
        db.artists.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    println!(
        "albums: {:?}",
        db.albums.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    println!(
        "genres: {:?}",
        db.genres.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    println!(
        "keys: {:?}",
        db.keys.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    println!(
        "colors: {:?}",
        db.colors.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    println!(
        "artwork: {:?}",
        db.artwork.iter().map(|a| &a.path).collect::<Vec<_>>()
    );
    print_problems(&db);
    Ok(())
}

fn fmt_ms(ms: u32) -> String {
    format!("{}:{:02}.{:03}", ms / 60000, ms / 1000 % 60, ms % 1000)
}

fn anlz(file: &Path) -> Result<()> {
    let bytes = std::fs::read(file)?;
    let a = AnlzFile::parse(&bytes)?;
    for s in &a.sections {
        println!(
            "{} header={}B body={}B",
            s.tag_str(),
            s.header.len(),
            s.body.len()
        );
    }
    if let Some(p) = a.path() {
        println!("\npath: {p}");
    }
    if let Some(grid) = a.beat_grid() {
        let grid = grid?;
        let first = grid.beats.first();
        println!(
            "beat grid: {} beats, first at {} ({:.2} BPM, beat {})",
            grid.beats.len(),
            first.map(|b| fmt_ms(b.time_ms)).unwrap_or_default(),
            first.map(|b| b.tempo as f64 / 100.0).unwrap_or(0.0),
            first.map(|b| b.beat_number).unwrap_or(0)
        );
    }
    let slot = |n: u32| match n {
        0 => "mem".to_string(),
        n => ((b'A' + (n - 1) as u8) as char).to_string(),
    };
    for list in a.ext_cue_lists()? {
        println!("PCO2 {:?}:", list.kind);
        for c in &list.cues {
            let lp = match c.cue_type {
                CueType::Loop => format!(" → {}", fmt_ms(c.loop_time_ms)),
                _ => String::new(),
            };
            println!(
                "  {:>3} {:?} {}{lp} color_id={} color={:?} comment={:?}",
                slot(c.hot_cue),
                c.cue_type,
                fmt_ms(c.time_ms),
                c.color_id,
                c.color,
                c.comment
            );
        }
    }
    for list in a.cue_lists()? {
        println!("PCOB {:?}: {} cues", list.kind, list.cues.len());
    }
    for s in &a.sections {
        if WaveformKind::from_tag(s.tag).is_some() {
            let w = Waveform::parse(s)?;
            println!("waveform {:?}: {} columns", w.kind, w.columns());
        }
    }
    Ok(())
}

fn human(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
    }
}

fn extract_cmd(
    usb: &Path,
    dest: &Path,
    names: &[String],
    mut opts: extract::ExtractOptions,
    dry_run: bool,
) -> Result<()> {
    let stick = Stick::open(usb)?;
    for name in names {
        let found: Vec<u32> = stick
            .db
            .playlists
            .iter()
            .filter(|p| !p.is_folder && p.name.eq_ignore_ascii_case(name))
            .map(|p| p.id)
            .collect();
        anyhow::ensure!(!found.is_empty(), "no playlist named {name:?} on the stick");
        opts.playlists.extend(found);
    }
    let plan = extract::plan(&stick, &opts);
    for pl in &plan.playlists {
        println!("📁 {}  ({} tracks)", pl.folder.display(), pl.files.len());
        if dry_run {
            for f in &pl.files {
                println!("    {}", f.file_name);
            }
        }
    }
    println!("{} files, {}", plan.total_files, human(plan.total_bytes));
    if !plan.missing.is_empty() {
        println!("⚠ {} missing on the stick:", plan.missing.len());
        for m in &plan.missing {
            println!("    {m}");
        }
    }
    if dry_run {
        return Ok(());
    }
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let report = extract::execute(&plan, dest, &opts, &cancel, |p| {
        eprint!(
            "\r[{}/{}] {} / {}  {:<60.60}",
            p.files_done,
            p.files_total,
            human(p.bytes_done),
            human(p.bytes_total),
            p.current
        );
    })?;
    eprintln!();
    println!(
        "✓ copied {}, cloned {}, linked {}, already there {}",
        report.copied, report.cloned, report.linked, report.skipped_existing
    );
    if !report.errors.is_empty() {
        println!("⚠ {} errors:", report.errors.len());
        for e in &report.errors {
            println!("    {e}");
        }
        anyhow::bail!("extract finished with errors");
    }
    Ok(())
}

fn verify_cmd(usb: &Path, players: &[PlayerArg]) -> Result<()> {
    let players: Vec<compat::Player> = if players.is_empty() {
        compat::Player::ALL.to_vec()
    } else {
        players.iter().map(|&p| p.into()).collect()
    };
    let stick = Stick::open(usb)?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let r = verify::verify(&stick, &players, &cancel, |i, n| {
        eprint!("\rchecking {i}/{n}")
    });
    eprintln!();
    println!("{} tracks, {} playlists", r.tracks, r.playlists);
    println!("filesystem: {}", r.filesystem.as_deref().unwrap_or("?"));
    for (p, why) in &r.filesystem_issues {
        println!("  ✗ {}: {why}", p.name());
    }
    println!("formats:");
    for (f, n) in &r.formats {
        println!("  {n:>5}  {f}");
    }
    if !r.db_problems.is_empty() {
        println!("\n✗ database problems:");
        for p in &r.db_problems {
            println!("  {p}");
        }
    }
    if !r.apple_double.is_empty() {
        println!(
            "\n⚠ {} macOS '._' files (show up as broken tracks when browsing folders)",
            r.apple_double.len()
        );
    }
    let (warnings, errors): (Vec<_>, Vec<_>) =
        r.issues.iter().partition(|i| i.problem.is_warning());
    if errors.is_empty() {
        println!("\n✓ no track errors");
    } else {
        println!("\n✗ {} track error(s):", errors.len());
        for i in &errors {
            print_issue(i);
        }
    }
    if !warnings.is_empty() {
        println!("\n⚠ {} warning(s):", warnings.len());
        for i in &warnings {
            print_issue(i);
        }
    }
    Ok(())
}

fn print_issue(i: &verify::TrackIssue) {
    let what = match &i.problem {
        verify::Problem::MissingAudio => "audio file missing".to_string(),
        verify::Problem::MissingAnalysis(m) => format!("analysis: {m}"),
        verify::Problem::SizeMismatch { database, actual } => {
            format!("file is {actual} B, database says {database} B (file changed after analysis)")
        }
        verify::Problem::BrokenTag { junk } => format!(
            "broken ID3 tag: {junk} stray bytes before the audio (players may refuse the file)"
        ),
        verify::Problem::WrongExtension { extension, actual } => {
            format!(".{extension} file is really {actual}")
        }
        verify::Problem::Unsupported { players, reason } => format!(
            "{reason} — {}",
            players
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        verify::Problem::Unreadable(e) => format!("unreadable: {e}"),
    };
    println!("\n  • {}", i.display);
    if let Some(a) = &i.audio {
        println!("    {}", a.describe());
    }
    println!("    {what}");
    println!("    {}", i.file_path);
    if !i.playlists.is_empty() {
        println!("    playlists: {}", i.playlists.join(", "));
    }
}

fn smart_cue_cmd(
    usb: &Path,
    only: Option<u32>,
    eval: bool,
    list_cued: bool,
    detr: Option<&Path>,
    detr_threshold: f32,
) -> Result<()> {
    // CUE-DETR results by audio path: candidate (time ms, score).
    let mut detr_cues: std::collections::HashMap<String, Vec<(u32, f32)>> = Default::default();
    if let Some(p) = detr {
        for line in std::fs::read_to_string(p)?.lines() {
            let v: serde_json::Value = serde_json::from_str(line)?;
            let (Some(path), Some(c)) = (v["path"].as_str(), v["candidates"].as_array()) else {
                continue;
            };
            detr_cues.insert(
                path.to_string(),
                c.iter()
                    .map(|x| {
                        (
                            (x["time"].as_f64().unwrap_or(0.0) * 1000.0) as u32,
                            x["score"].as_f64().unwrap_or(0.0) as f32,
                        )
                    })
                    .collect(),
            );
        }
    }
    let (mut d_user_hit, mut d_total, mut d_hit, mut d_tracks) = (0, 0, 0, 0);
    let (mut m_plan_total, mut m_plan_hit) = (0, 0);
    use analysis::smart_cue::{
        Input, analyze, bands_from_color, bands_from_three_band, hot_cue_plan,
    };
    use pioneer_formats::anlz::{read_cue_points, tag};
    let stick = Stick::open(usb)?;
    let (mut user_total, mut user_hit, mut sugg_total, mut sugg_hit, mut tracks) = (0, 0, 0, 0, 0);
    let (mut plan_hit, mut plan_total) = (0, 0);
    for t in &stick.db.tracks {
        if only.is_some_and(|id| id != t.id) {
            continue;
        }
        let dat_path = stick.resolve(&t.analyze_path);
        let Ok(dat) = std::fs::read(&dat_path)
            .map_err(anyhow::Error::from)
            .and_then(|b| Ok(AnlzFile::parse(&b)?))
        else {
            continue;
        };
        let ext = std::fs::read(dat_path.with_extension("EXT"))
            .ok()
            .and_then(|b| AnlzFile::parse(&b).ok());
        let two = std::fs::read(dat_path.with_extension("2EX"))
            .ok()
            .and_then(|b| AnlzFile::parse(&b).ok());
        let columns = match (&two, &ext) {
            (Some(f), _) if f.section(tag::WAVE_3BAND_SCROLL).is_some() => {
                bands_from_three_band(&f.section(tag::WAVE_3BAND_SCROLL).unwrap().body)
            }
            (_, Some(f)) if f.section(tag::WAVE_COLOR_SCROLL).is_some() => {
                bands_from_color(&f.section(tag::WAVE_COLOR_SCROLL).unwrap().body)
            }
            _ => continue,
        };
        let beats: Vec<(u32, u16)> = dat
            .beat_grid()
            .and_then(|g| g.ok())
            .map(|g| g.beats.iter().map(|b| (b.time_ms, b.beat_number)).collect())
            .unwrap_or_default();
        let input = Input {
            beats: &beats,
            columns: &columns,
            rate: 150.0,
            bpm: t.bpm() as f32,
        };
        let s = analyze(&input);
        let user: Vec<u32> = read_cue_points(&dat, ext.as_ref())
            .unwrap_or_default()
            .iter()
            .filter(|c| c.slot > 0)
            .map(|c| c.time_ms)
            .collect();
        if (eval || list_cued) && user.is_empty() {
            continue;
        }
        if list_cued {
            println!("{}", stick.audio_path(t).display());
            continue;
        }
        tracks += 1;
        let bar_ms = (4.0 * 60000.0 / t.bpm().max(1.0)) as u32;
        let near = |a: u32, b: u32| a.abs_diff(b) <= bar_ms;
        if let Some(c) = detr_cues.get(&stick.audio_path(t).display().to_string()) {
            // Keep the strongest candidate within a bar, above the threshold.
            let mut picked: Vec<(u32, f32)> = Vec::new();
            let mut sorted: Vec<(u32, f32)> = c
                .iter()
                .copied()
                .filter(|x| x.1 >= detr_threshold)
                .collect();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            for x in sorted {
                if !picked.iter().any(|p| near(p.0, x.0)) {
                    picked.push(x);
                }
            }
            d_tracks += 1;
            d_total += picked.len();
            d_hit += picked
                .iter()
                .filter(|p| user.iter().any(|&u| near(p.0, u)))
                .count();
            d_user_hit += user
                .iter()
                .filter(|&&u| picked.iter().any(|p| near(p.0, u)))
                .count();
            let cands: Vec<analysis::smart_cue::AiCandidate> = c
                .iter()
                .map(|&(time_ms, score)| analysis::smart_cue::AiCandidate { time_ms, score })
                .collect();
            let merged = analysis::smart_cue::with_ai(&input, &s, &cands, detr_threshold);
            let plan = hot_cue_plan(&merged);
            m_plan_total += plan.len();
            m_plan_hit += user
                .iter()
                .filter(|&&u| plan.iter().any(|x| near(x.time_ms, u)))
                .count();
            if only.is_some() {
                for x in &merged {
                    let mark = if user.iter().any(|&u| near(x.time_ms, u)) {
                        "✓"
                    } else {
                        " "
                    };
                    let ai = if x.ai { "AI" } else { "  " };
                    println!(
                        "  {mark} {ai} {:>8}  bar {:>3}  {:<12} {:.2}",
                        fmt_ms(x.time_ms),
                        x.bar,
                        x.label,
                        x.confidence
                    );
                }
            }
        }
        if eval {
            user_total += user.len();
            user_hit += user
                .iter()
                .filter(|&&u| s.iter().any(|x| near(x.time_ms, u)))
                .count();
            sugg_total += s.len();
            sugg_hit += s
                .iter()
                .filter(|x| user.iter().any(|&u| near(x.time_ms, u)))
                .count();
            let plan = hot_cue_plan(&s);
            plan_total += plan.len();
            plan_hit += user
                .iter()
                .filter(|&&u| plan.iter().any(|x| near(x.time_ms, u)))
                .count();
        }
        if !eval || only.is_some() {
            println!(
                "#{} {} – {}",
                t.id,
                stick.db.artist_name(t.artist_id).unwrap_or(""),
                t.title
            );
            for x in &s {
                let mark = if user.iter().any(|&u| near(x.time_ms, u)) {
                    "✓"
                } else {
                    " "
                };
                println!(
                    "  {mark} {:>8}  bar {:>3}  {:<8} {:.2}",
                    fmt_ms(x.time_ms),
                    x.bar,
                    x.label,
                    x.confidence
                );
            }
            if !user.is_empty() {
                let mut u = user.clone();
                u.sort();
                println!(
                    "    your hot cues: {}",
                    u.iter().map(|&m| fmt_ms(m)).collect::<Vec<_>>().join(", ")
                );
            }
        }
    }
    if eval {
        println!(
            "\n{tracks} tracks with your hot cues: Smart Cue found {user_hit}/{user_total} of your cues ({:.0} %); \
             {sugg_hit}/{sugg_total} suggestions sit on one of your cues ({:.0} %)",
            100.0 * user_hit as f64 / user_total.max(1) as f64,
            100.0 * sugg_hit as f64 / sugg_total.max(1) as f64
        );
        println!(
            "hot cue plan (max 8 per track, {plan_total} cues): covers {plan_hit}/{user_total} of your cues ({:.0} %)",
            100.0 * plan_hit as f64 / user_total.max(1) as f64
        );
        if d_tracks > 0 {
            println!(
                "CUE-DETR (score ≥ {detr_threshold}, {d_tracks} tracks): found {d_user_hit} of your cues; \
                 {d_hit}/{d_total} of its cues sit on one of yours ({:.0} %)",
                100.0 * d_hit as f64 / d_total.max(1) as f64
            );
            println!(
                "Smart Cue + CUE-DETR hot cue plan ({m_plan_total} cues): covers {m_plan_hit}/{user_total} of your cues ({:.0} %)",
                100.0 * m_plan_hit as f64 / user_total.max(1) as f64
            );
        }
    }
    Ok(())
}
