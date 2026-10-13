use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pioneer_formats::anlz::{AnlzFile, CueType, Waveform, WaveformKind};
use pioneer_formats::pdb::{Database, table};

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
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Usb { path } => usb(&path),
        Cmd::Pdb { file } => pdb(&file),
        Cmd::Anlz { file } => anlz(&file),
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
