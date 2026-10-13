//! Safe file and folder names for macOS (APFS / HFS+).

use unicode_normalization::UnicodeNormalization;

/// Longest name component we produce, in UTF-8 bytes (APFS allows 255
/// UTF-8 bytes; leave room for " (99)" and the extension).
const MAX_NAME_BYTES: usize = 200;

/// Make `name` usable as a single path component:
/// NFC-normalised, no `/` or `:` (Finder shows `:` as `/`), no control
/// characters, no leading dot (hidden file), no trailing dots or spaces,
/// at most [`MAX_NAME_BYTES`] bytes, never empty.
pub fn sanitize(name: &str) -> String {
    let mut s: String = name
        .nfc()
        .map(|c| match c {
            '/' | ':' | '\\' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    while s.starts_with('.') {
        s.remove(0);
    }
    truncate_bytes(&mut s, MAX_NAME_BYTES);
    let trimmed = s.trim_end_matches(['.', ' ']).len();
    s.truncate(trimmed);
    if s.is_empty() { "_".to_string() } else { s }
}

fn truncate_bytes(s: &mut String, max: usize) {
    if s.len() <= max {
        return;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
}

/// Split `file.mp3` into (`file`, `.mp3`).
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= 6 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// `stem.ext`, `stem (2).ext`, `stem (3).ext`, … until `taken` says no.
pub fn unique(stem: &str, ext: &str, mut taken: impl FnMut(&str) -> bool) -> String {
    let first = format!("{stem}{ext}");
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|c| !taken(c))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize("AC/DC: Live"), "AC-DC- Live");
        assert_eq!(sanitize("..hidden"), "hidden");
        assert_eq!(sanitize("name. . "), "name");
        assert_eq!(sanitize(""), "_");
        assert_eq!(sanitize("a\tb\nc"), "a b c");
        // decomposed "é" becomes the composed form
        assert_eq!(sanitize("Beyonce\u{301}"), "Beyoncé");
        let long = "ž".repeat(300);
        let s = sanitize(&long);
        assert!(s.len() <= 200 && s.chars().all(|c| c == 'ž'));
    }

    #[test]
    fn splits_and_uniques() {
        assert_eq!(split_ext("a.b.mp3"), ("a.b", ".mp3"));
        assert_eq!(split_ext("noext"), ("noext", ""));
        assert_eq!(split_ext(".hidden"), (".hidden", ""));
        let taken = ["x.mp3", "x (2).mp3"];
        assert_eq!(unique("x", ".mp3", |n| taken.contains(&n)), "x (3).mp3");
    }
}
