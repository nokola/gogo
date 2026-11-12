//! A build held in memory, for the hot reload tests: pages compiled the
//! way `build_pages` compiles them (one key table, one picture table, one
//! slice table), handed back as the tables a program built from them
//! holds (`BuildTables`), and the page files as a reload reads them.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gogo_ui::page::{AtlasShape, ImageSlice};
use gogo_ui::reload::{BuildTables, BuiltAtlas, BuiltPage, BuiltShape, Sources, TextStamp};
use gogo_ui::{CompileOptions, Images, Keys, Page, StaticPage, compile_with};

/// A PNG's first 24 bytes: all an `atlas` line reads of a picture.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    b.extend_from_slice(b"IHDR");
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b
}

fn leak<T: Clone>(v: &[T]) -> &'static [T] {
    Box::leak(v.to_vec().into_boxed_slice())
}

fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// `page` as a build emits it: the same tables as `'static` slices, with
/// the build's whole slice table.
pub fn as_built(page: &Page, slices: &'static [ImageSlice]) -> &'static StaticPage {
    let strings: Vec<&'static str> = page.strings.iter().map(|s| leak_str(s)).collect();
    Box::leak(Box::new(StaticPage {
        nodes: leak(&page.nodes),
        strings: leak(&strings),
        props: leak(&page.props),
        values: leak(&page.values),
        whens: leak(&page.whens),
        handlers: leak(&page.handlers),
        theme: leak(&page.theme),
        kind_defaults: leak(&page.kind_defaults),
        arrangements: leak(&page.arrangements),
        transitions: leak(&page.transitions),
        prop_transitions: leak(&page.prop_transitions),
        anims: leak(&page.anims),
        anim_heads: leak(&page.anim_heads),
        choices: leak(&page.choices),
        repeats: leak(&page.repeats),
        bind_sites: leak(&page.bind_sites),
        custom_states: leak(&page.custom_states),
        key_claims: leak(&page.key_claims),
        drivers: leak(&page.drivers),
        behaviors: leak(&page.behaviors),
        value_follows: leak(&page.value_follows),
        slices,
    }))
}

/// A build and the folder its pages are read from while the program runs.
pub struct Kit {
    pub tables: &'static BuildTables<'static>,
    /// The page files by name, as a reload reads them. They start as the
    /// build read them; `save` changes one.
    files: Arc<Mutex<HashMap<String, String>>>,
}

impl Kit {
    /// The built page `file` (`page.gogo`).
    pub fn built(&self, file: &str) -> &'static StaticPage {
        self.tables.pages.iter().find(|p| p.file == file).unwrap_or_else(|| panic!("no page {file} in the build")).page
    }

    /// A running copy of the built page `file`.
    pub fn page(&self, file: &str) -> Page {
        Page::from_static(self.built(file))
    }

    /// Where a reload reads the files from.
    pub fn sources(&self) -> Sources {
        let files = self.files.clone();
        Sources::read_with(move |rel: &str| files.lock().unwrap().get(rel).cloned())
    }

    /// The file's text as it is now.
    pub fn text(&self, file: &str) -> String {
        self.files.lock().unwrap().get(file).cloned().unwrap_or_else(|| panic!("no file {file}"))
    }

    /// Saves `text` as `file`.
    pub fn save(&self, file: &str, text: &str) {
        self.files.lock().unwrap().insert(file.to_string(), text.to_string());
    }

    /// Saves `file` with `old` replaced by `new` (`old` must be in it).
    pub fn edit(&self, file: &str, old: &str, new: &str) {
        let text: String = self.text(file);
        assert!(text.contains(old), "{file} holds no `{old}`");
        self.save(file, &text.replace(old, new));
    }

    /// A file's read for `recompile`.
    pub fn loader(&self) -> impl Fn(&str) -> Option<String> + 'static {
        let files = self.files.clone();
        move |rel: &str| files.lock().unwrap().get(rel).cloned()
    }

    /// `file` as it is now, compiled against the build.
    pub fn recompile(&self, file: &str) -> Result<Page, Vec<gogo_ui::Diagnostic>> {
        gogo_ui::reload::recompile(&self.text(file), file, &self.loader(), self.tables)
    }
}

/// Builds `pages` (file name, text). `others` are the files they `uses`;
/// `pictures` the files an `atlas` line reads. Keys are required
/// (`require_keys`), as in a build that ships in several languages.
pub fn build(pages: &[(&str, &str)], others: &[(&str, &str)], pictures: &[(&str, Vec<u8>)]) -> Kit {
    let files: HashMap<String, String> = pages.iter().chain(others).map(|(name, text)| (name.to_string(), text.to_string())).collect();
    let keys = RefCell::new(Keys::default());
    let images = RefCell::new(Images::default());
    let loader = |rel: &str| files.get(rel).cloned();
    let read = |path: &str| pictures.iter().find(|(name, _)| *name == path).map(|(_, bytes)| bytes.clone()).ok_or_else(|| "no such file".to_string());
    let mut compiled_pages: Vec<(&str, Page, Vec<String>, u64)> = Vec::new();
    for (name, text) in pages {
        let compiled = compile_with(text, &loader, CompileOptions { keys: Some(&keys), images: Some(&images), assets: Some(&read), require_keys: true, source_name: name, ..CompileOptions::default() });
        assert!(!gogo_ui::has_errors(&compiled.diags), "{name}: {:#?}", compiled.diags);
        let mut stamp = TextStamp::of(text);
        for used in &compiled.uses {
            stamp.used(used, files.get(used).map(String::as_str));
        }
        compiled_pages.push((name, compiled.page, compiled.uses, stamp.finish()));
    }
    let keys: Keys = keys.into_inner();
    let images: Images = images.into_inner();
    let key_rows: Vec<(&'static str, &'static str)> = keys.names.iter().zip(&keys.english).map(|(n, e)| (leak_str(n), leak_str(e))).collect();
    let image_rows: Vec<(&'static str, Option<&'static [u8]>)> = images.names.iter().map(|n| (leak_str(n), None)).collect();
    let slices: &'static [ImageSlice] = leak(&images.slices);
    let atlases: Vec<BuiltAtlas<'static>> = images
        .atlases
        .iter()
        .map(|a| {
            let shape: BuiltShape<'static> = match &a.shape {
                AtlasShape::Grid(keys) => BuiltShape::Grid(leak(keys)),
                AtlasShape::Named(list) => BuiltShape::Named(leak(&list.iter().map(|(n, k)| (leak_str(n), *k)).collect::<Vec<_>>())),
                AtlasShape::Whole(k) => BuiltShape::Whole(*k),
            };
            BuiltAtlas { name: leak_str(&a.name), said: leak(&a.said), shape }
        })
        .collect();
    let built: Vec<BuiltPage<'static>> = compiled_pages
        .iter()
        .map(|(name, page, uses, text)| BuiltPage { file: leak_str(name), page: as_built(page, slices), uses: leak(&uses.iter().map(|u| leak_str(u)).collect::<Vec<_>>()), text: *text })
        .collect();
    let tables: &'static BuildTables<'static> = Box::leak(Box::new(BuildTables { keys: leak(&key_rows), images: leak(&image_rows), slices, atlases: leak(&atlases), code_controls: &[], behaviors: &[], require_keys: true, pages: leak(&built) }));
    Kit { tables, files: Arc::new(Mutex::new(files)) }
}
