//! Translations of the texts the Rust side shows the DJ: errors, job
//! labels and progress, the menu. The same format as the app's frontend
//! (`app/src/locales`): one JSON file per part, all languages side by side,
//! `{name}` placeholders, plural forms picked by `count`. Czech is the
//! reference; `app/scripts/check-i18n.mjs` and the tests here check the
//! other languages have every key.
//!
//! The language is process-wide: the app sets it from the DJ's choice.
//!
//! ```
//! i18n::set_lang("en");
//! assert_eq!(i18n::t!("app.settingsMenu"), "Settings…");
//! ```

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde_json::Value;

pub const LANGS: [&str; 3] = ["cs", "en", "de"];

/// The files in `locales`, by namespace.
const FILES: &[(&str, &str)] = &[("app", include_str!("../locales/app.json"))];

static DICTS: LazyLock<Vec<(&'static str, Value)>> = LazyLock::new(|| {
    FILES
        .iter()
        .map(|(ns, json)| (*ns, serde_json::from_str(json).unwrap_or_else(|e| panic!("locales/{ns}.json: {e}"))))
        .collect()
});

static LANG: AtomicU8 = AtomicU8::new(0);

/// Use `lang` (`cs`, `en`, `de`) from now on; anything else is English.
pub fn set_lang(lang: &str) {
    let i = LANGS.iter().position(|l| *l == lang).unwrap_or(1);
    LANG.store(i as u8, Ordering::Relaxed);
}

pub fn lang() -> &'static str {
    LANGS[LANG.load(Ordering::Relaxed) as usize]
}

fn lookup(lang: &str, key: &str) -> Option<&'static Value> {
    let (ns, path) = key.split_once('.')?;
    let (_, dict) = DICTS.iter().find(|(n, _)| *n == ns)?;
    path.split('.').try_fold(dict.get(lang)?, |v, part| v.get(part))
}

/// The plural form for `n`: Czech one/few/other, English and German one/other.
fn plural(lang: &str, n: i64) -> &'static str {
    match (lang, n) {
        (_, 1) => "one",
        ("cs", 2..=4) => "few",
        _ => "other",
    }
}

/// The text for `key` (`namespace.path`) in the current language, with
/// `{name}` placeholders from `args`; plural forms by the `count` arg.
/// Missing in the language: English; missing altogether: the key.
pub fn translate(key: &str, args: &[(&str, String)]) -> String {
    let lang = lang();
    let Some(v) = lookup(lang, key).or_else(|| lookup("en", key)) else {
        return key.to_string();
    };
    let text = match v {
        Value::String(s) => s.as_str(),
        Value::Object(forms) => {
            let n = args.iter().find(|(k, _)| *k == "count").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
            forms
                .get(plural(lang, n))
                .or_else(|| forms.get("other"))
                .and_then(Value::as_str)
                .unwrap_or(key)
        }
        _ => key,
    };
    let mut out = text.to_string();
    for (k, v) in args {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// `t!("ns.key")`, `t!("ns.key", name = value, count = n)`: [`translate`].
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::translate($key, &[])
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::translate($key, &[$((stringify!($name), $value.to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every language has the keys Czech has, with the same kind of value.
    #[test]
    fn languages_have_every_key() {
        fn walk(ns: &str, path: &str, cs: &Value, other: &Value, lang: &str, missing: &mut Vec<String>) {
            if let Value::Object(map) = cs {
                if map.contains_key("other") {
                    if !other.is_object() {
                        missing.push(format!("{lang} {ns}.{path}: should be plural"));
                    }
                    return;
                }
                for (k, v) in map {
                    let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                    match other.get(k) {
                        Some(o) => walk(ns, &p, v, o, lang, missing),
                        None => missing.push(format!("{lang} {ns}.{p}")),
                    }
                }
            }
        }
        let mut missing = Vec::new();
        for (ns, dict) in DICTS.iter() {
            for lang in &LANGS[1..] {
                walk(ns, "", &dict["cs"], &dict[*lang], lang, &mut missing);
            }
        }
        assert!(missing.is_empty(), "missing translations:\n{}", missing.join("\n"));
    }

    #[test]
    fn plural_forms_and_placeholders() {
        assert_eq!(plural("cs", 1), "one");
        assert_eq!(plural("cs", 3), "few");
        assert_eq!(plural("cs", 5), "other");
        assert_eq!(plural("de", 3), "other");
        assert_eq!(translate("nope.key", &[]), "nope.key");
    }
}
