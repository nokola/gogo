//! Translations: which string a `text="x"/key` shows right now.
//! Needed because an app can switch language while it runs and can load
//! its translations after the build (from a file, from a server) — the
//! table is data that arrives later, so a key can't resolve to a string at
//! compile time. Limit: this file knows keys and strings, not files — the
//! format a translation is stored in is the host's, and RTL/line-breaking
//! is the text engine's. Mechanism: `Keys` is the build-time interner (one id space
//! for every page, `build_pages`), `Locale` the runtime table indexed by
//! that id: one `Vec` index and one branch per read, no hashing, no
//! allocation — loads allocate, reads never.

use std::collections::HashMap;

use crate::page::{KeyId, NO_KEY, NO_STR, Page, StrId};

/// The build's key table, filled by the compiler as pages name keys:
/// one id per name across EVERY file (`ok` in two pages is one key), and
/// the G8001 rule — one key, one English string — checked here so it
/// holds across files, not per file.
#[derive(Clone, Debug, Default)]
pub struct Keys {
    /// By id: the key as written (`pickColorHint`).
    pub names: Vec<String>,
    /// By id: the English literal the first page wrote for it.
    pub english: Vec<String>,
    /// By id: the file the key was first seen in (`CompileOptions::
    /// source_name`; `""` = unknown) — G8001 names it, so the author knows
    /// which of two files to fix.
    pub origins: Vec<String>,
    index: HashMap<String, KeyId>,
}

impl Keys {
    /// The id for `name`, new or existing. `Err((previous English, its
    /// origin))` when the name already carries a different English string
    /// (G8001) — the id is still assigned, the caller reports.
    pub fn intern(&mut self, name: &str, english: &str, origin: &str) -> Result<KeyId, (String, String)> {
        if let Some(&id) = self.index.get(name) {
            let prev = &self.english[id];
            return if prev == english { Ok(id) } else { Err((prev.clone(), self.origins[id].clone())) };
        }
        let id = KeyId::from_index(self.names.len());
        self.names.push(name.to_string());
        self.english.push(english.to_string());
        self.origins.push(origin.to_string());
        self.index.insert(name.to_string(), id);
        Ok(id)
    }

    /// A table pre-seeded from a build's `KEYS`, so a page compiled at
    /// runtime (hot reload) with `CompileOptions { keys: Some(..) }` bakes
    /// the SAME ids the build did — else its `/ok` would index another
    /// key's string.
    pub fn from_table(table: &[(&str, &str)]) -> Keys {
        let mut k = Keys::default();
        for (name, english) in table {
            let _ = k.intern(name, english, "");
        }
        k
    }

    pub fn get(&self, name: &str) -> Option<KeyId> {
        self.index.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// A build-emitted key (the `Key` enum the build writes for the host) —
/// the only thing `Runtime::set_key` takes, so a key the pages never
/// declare is a rustc error, not a runtime miss.
pub trait KeyName: Copy {
    fn id(self) -> KeyId;
}

/// The strings one language shows, by `KeyId`. Built once per load (two
/// allocations: the index and one arena holding every string), read with
/// one index and one branch. A key the load did not cover falls back to
/// its English from the build's `KEYS` table.
#[derive(Clone, Debug, PartialEq)]
pub struct Locale {
    /// The build's `(name, English)` per key — the fallback.
    english: &'static [(&'static str, &'static str)],
    /// Per key: (start, len) into `arena`; `len == 0` = not translated.
    over: Vec<(u32, u32)>,
    arena: String,
}

impl Locale {
    /// English only — every key shows its literal.
    pub fn english(english: &'static [(&'static str, &'static str)]) -> Locale {
        Locale { english, over: Vec::new(), arena: String::new() }
    }

    /// One language: `pairs` = (key, translated string). A key listed
    /// twice keeps the LAST entry (a fallback chain is flattened by
    /// listing the base language first, the overriding one after). Ids
    /// past the `english` table are ignored.
    pub fn build(english: &'static [(&'static str, &'static str)], pairs: &[(KeyId, &str)]) -> Locale {
        let n = english.len();
        let total: usize = pairs.iter().filter(|(k, _)| k.index() < n).map(|(_, s)| s.len()).sum();
        let mut arena = String::with_capacity(total);
        let mut over = vec![(0u32, 0u32); n];
        for (k, s) in pairs {
            if k.index() >= n {
                continue;
            }
            let start = arena.len() as u32;
            arena.push_str(s);
            over[*k] = (start, s.len() as u32);
        }
        Locale { english, over, arena }
    }

    /// Replaces one key's string (a translator's live edit, a dev reload
    /// of one term). Appends to the arena — the old bytes stay until the
    /// next `build`; not for the hot path.
    pub fn set(&mut self, key: KeyId, s: &str) {
        let n = self.english.len();
        if key.index() >= n {
            return;
        }
        if self.over.len() < n {
            self.over.resize(n, (0, 0));
        }
        let start = self.arena.len() as u32;
        self.arena.push_str(s);
        self.over[key] = (start, s.len() as u32);
    }

    /// The string `key` shows: its translation, else its English. `None`
    /// for a key this build does not know.
    #[inline]
    pub fn text(&self, key: KeyId) -> Option<&str> {
        if let Some(&(start, len)) = self.over.get(key.index()) {
            if len > 0 {
                return Some(&self.arena[start as usize..(start + len) as usize]);
            }
        }
        self.english.get(key.index()).map(|(_, e)| *e)
    }

    pub fn is_translated(&self, key: KeyId) -> bool {
        matches!(self.over.get(key.index()), Some(&(_, len)) if len > 0)
    }

    /// How many keys carry a translation (the shipped `PercentComplete`
    /// numerator).
    pub fn translated(&self) -> usize {
        self.over.iter().filter(|(_, len)| *len > 0).count()
    }

    pub fn key_count(&self) -> usize {
        self.english.len()
    }

    /// The shipped `gogo.translate(key, fallback)` for code-side strings
    /// (toasts, formatted messages): the translation for the key NAMED
    /// `key`, else `fallback` — an empty fallback means the key itself
    /// (the shipped rule). Needed because code has a name, not a `KeyId`:
    /// the pages' `/key` path bakes ids at build time, a toast's key is a
    /// string in the code. A name lookup is a scan of the build's table —
    /// a toast a minute, not a label a frame; the per-frame reads stay on
    /// `text(KeyId)`. A key this build does not know shows the fallback.
    pub fn translate<'a>(&'a self, key: &'a str, fallback: &'a str) -> &'a str {
        let fallback: &str = if fallback.is_empty() { key } else { fallback };
        match self.english.iter().position(|(name, _)| *name == key) {
            Some(id) if self.is_translated(KeyId::from_index(id)) => self.text(KeyId::from_index(id)).unwrap_or(fallback),
            _ => fallback,
        }
    }
}

/// What a `VText { text, key }` shows: the locale's string for `key` when
/// there is a locale and it knows the key, else the literal `text`. The one
/// place layout (to measure) and the renderer (to shape) agree on — both
/// call this, so a label is never measured in one language and drawn in
/// another.
#[inline]
pub fn display<'a>(page: &'a Page, locale: Option<&'a Locale>, text: StrId, key: KeyId) -> &'a str {
    if key != NO_KEY {
        if let Some(s) = locale.and_then(|l| l.text(key)) {
            return s;
        }
    }
    if text == NO_STR { "" } else { page.str(text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TABLE: &[(&str, &str)] = &[("ok", "OK"), ("cancel", "Cancel"), ("size", "Size")];

    #[test]
    fn keys_are_one_id_per_name_and_g8001_across_calls() {
        let mut k = Keys::default();
        assert_eq!(k.intern("ok", "OK", "a.gogo"), Ok(KeyId::new(0)));
        assert_eq!(k.intern("cancel", "Cancel", "a.gogo"), Ok(KeyId::new(1)));
        assert_eq!(k.intern("ok", "OK", "b.gogo"), Ok(KeyId::new(0)), "same name, same English = same id");
        assert_eq!(k.intern("ok", "Okay", "b.gogo"), Err(("OK".to_string(), "a.gogo".to_string())), "a second English for one key is the G8001 drift, naming the first file");
        assert_eq!(k.get("cancel"), Some(KeyId::new(1)));
        assert_eq!(k.len(), 2);
    }

    #[test]
    fn build_reads_translations_and_falls_back_to_english() {
        let l = Locale::build(TABLE, &[(KeyId::new(1), "Abbrechen"), (KeyId::new(99), "ignored")]);
        assert_eq!(l.text(KeyId::new(0)), Some("OK"), "untranslated = English");
        assert_eq!(l.text(KeyId::new(1)), Some("Abbrechen"));
        assert_eq!(l.text(KeyId::new(99)), None, "a key the build does not know");
        assert_eq!(l.translated(), 1);
        assert!(l.is_translated(KeyId::new(1)) && !l.is_translated(KeyId::new(0)));
    }

    #[test]
    fn later_pairs_win_and_set_replaces() {
        let mut l = Locale::build(TABLE, &[(KeyId::new(0), "Ok (base)"), (KeyId::new(0), "Ok (dialect)")]);
        assert_eq!(l.text(KeyId::new(0)), Some("Ok (dialect)"), "a fallback chain flattens base-first, override-last");
        l.set(KeyId::new(2), "Größe");
        assert_eq!(l.text(KeyId::new(2)), Some("Größe"));
        l.set(KeyId::new(2), "");
        assert_eq!(l.text(KeyId::new(2)), Some("Size"), "an empty string un-translates the key");
        let mut e = Locale::english(TABLE);
        e.set(KeyId::new(1), "Annuler");
        assert_eq!(e.text(KeyId::new(1)), Some("Annuler"), "set works on an English-only table too");
    }

    #[test]
    fn translate_looks_a_code_key_up_by_name_and_falls_back() {
        let l = Locale::build(TABLE, &[(KeyId::new(1), "Abbrechen")]);
        assert_eq!(l.translate("cancel", "Cancel"), "Abbrechen");
        assert_eq!(l.translate("ok", "OK"), "OK", "a known key with no translation shows the caller's English");
        assert_eq!(l.translate("tapBack", "Tap Back to exit full screen"), "Tap Back to exit full screen", "an unknown key shows the fallback");
        assert_eq!(l.translate("tapBack", ""), "tapBack", "no fallback = the key itself (the shipped gogo.translate rule)");
        assert_eq!(Locale::english(TABLE).translate("cancel", "Cancel"), "Cancel");
    }

    #[test]
    fn display_prefers_the_locale_and_keeps_literals() {
        let mut page = Page::default();
        let hello = page.intern("Hello");
        let l = Locale::build(TABLE, &[(KeyId::new(0), "Okay!")]);
        assert_eq!(display(&page, Some(&l), hello, KeyId::new(0)), "Okay!");
        assert_eq!(display(&page, Some(&l), hello, KeyId::new(1)), "Cancel", "known key, no translation = the table's English");
        assert_eq!(display(&page, Some(&l), hello, KeyId::new(7)), "Hello", "unknown key = the page's literal");
        assert_eq!(display(&page, Some(&l), hello, NO_KEY), "Hello", "a literal ignores the locale");
        assert_eq!(display(&page, None, hello, KeyId::new(0)), "Hello", "no locale = the literal");
        assert_eq!(display(&page, None, NO_STR, KeyId::new(0)), "", "a code-set key with no locale shows nothing, not a panic");
    }
}
