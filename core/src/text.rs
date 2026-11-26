//! The words the menus and windows show, in the system's language when Snowhush speaks it, English otherwise.
//! The texts live in `locales/`, the same files the Mac app ships as its `.lproj` tables.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::simulation::Mode;

pub(crate) const LOCALES: [(&str, &str); 9] = [
    ("en", include_str!("../../locales/en.strings")),
    ("de", include_str!("../../locales/de.strings")),
    ("es", include_str!("../../locales/es.strings")),
    ("fr", include_str!("../../locales/fr.strings")),
    ("ja", include_str!("../../locales/ja.strings")),
    ("pt", include_str!("../../locales/pt.strings")),
    ("ru", include_str!("../../locales/ru.strings")),
    ("uk", include_str!("../../locales/uk.strings")),
    ("zh-Hans", include_str!("../../locales/zh-Hans.strings")),
];

static TABLES: [OnceLock<HashMap<String, String>>; LOCALES.len()] = [const { OnceLock::new() }; LOCALES.len()];
static CHOSEN: AtomicUsize = AtomicUsize::new(0);

/// Speaks the first of the user's languages, most preferred first as BCP 47 or POSIX tags ("pt-BR", "ru_RU.UTF-8"),
/// that Snowhush has. Changing the selection affects subsequent lookups.
pub fn choose<S: AsRef<str>>(preferred: &[S]) {
    if let Some(index) = preferred.iter().find_map(|tag| language_index(tag.as_ref())) {
        CHOSEN.store(index, Ordering::Relaxed);
    }
}

/// The language spoken, as its `locales/` name: "en", "pt", "zh-Hans".
pub fn code() -> &'static str {
    LOCALES[CHOSEN.load(Ordering::Relaxed)].0
}

/// The text for `key`, falling back to English.
pub fn tr(key: &str) -> &'static str {
    let chosen = CHOSEN.load(Ordering::Relaxed);
    table(chosen).get(key).or_else(|| table(0).get(key)).map_or("", String::as_str)
}

fn table(index: usize) -> &'static HashMap<String, String> {
    TABLES[index].get_or_init(|| parse(LOCALES[index].1))
}

/// The text for `prefix` and what falls, in `Mode::ALL`'s order: "windows.snow.autumn".
pub fn kinds(prefix: &str, modes: &[Mode]) -> &'static str {
    let mut key = prefix.to_string();
    for mode in Mode::ALL.iter().filter(|mode| modes.contains(mode)) {
        key.push('.');
        key.push_str(mode_key(*mode));
    }
    tr(&key)
}

/// `tr(key)` with each `{name}` filled in.
pub fn fill(key: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(tr(key).to_string(), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

fn mode_key(mode: Mode) -> &'static str {
    match mode {
        Mode::Snow => "snow",
        Mode::Autumn => "autumn",
        Mode::Rain => "rain",
        Mode::Blossom => "blossom",
    }
}

/// The language for a tag, with its table. Traditional Chinese has none of its own and falls back to English rather
/// than to Simplified.
#[cfg(test)]
pub(crate) fn language(tag: &str) -> Option<(&'static str, &'static str)> {
    language_index(tag).map(|index| LOCALES[index])
}

fn language_index(tag: &str) -> Option<usize> {
    let tag = tag.split(['.', '@']).next()?.replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    let name = match parts.next()? {
        "zh" => {
            let rest: Vec<&str> = parts.collect();
            let traditional = rest.iter().any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo"));
            if traditional { return None } else { "zh-Hans" }
        }
        primary => primary,
    };
    LOCALES.iter().position(|(code, _)| *code == name)
}

/// Reads an Apple `.strings` table: `"key" = "value";` lines, with `/* */` and `//` comments.
pub(crate) fn parse(text: &str) -> HashMap<String, String> {
    let mut table = HashMap::new();
    let mut chars = text.chars().peekable();
    let mut strings = Vec::new();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut string = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => string.push('\n'),
                            Some('t') => string.push('\t'),
                            Some(other) => string.push(other),
                            None => {}
                        },
                        other => string.push(other),
                    }
                }
                strings.push(string);
            }
            ';' => {
                if let [key, value] = strings.as_slice() {
                    table.insert(key.clone(), value.clone());
                }
                strings.clear();
            }
            _ => {}
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_languages_preserves_previously_returned_strings() {
        choose(&["ru"]);
        let russian = tr("settings");
        assert_eq!(russian, "Настройки");
        choose(&["en"]);
        assert_eq!(code(), "en");
        assert_eq!(tr("settings"), "Settings");
        assert_eq!(russian, "Настройки");
    }
}
