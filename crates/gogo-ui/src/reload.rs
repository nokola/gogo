//! Hot reload: a page follows its `.gogo` file while the app runs.
//!
//! A host makes one [`Reloader`] and calls three things on it:
//!
//! ```ignore
//! // once, at start
//! let mut reload = Reloader::new(&ui::BUILD, Sources::dir("ui"));
//! // when a page opens
//! let mut page: Page = reload.open(&ui::settings::PAGE);
//! // before the page's frame
//! match reload.refresh(&mut page, &mut runtime) {
//!     Ok(true) => { /* the page shows its file again: drop what the host kept about its layout */ }
//!     Ok(false) => {}
//!     Err(why) => eprintln!("{why}"),
//! }
//! ```
//!
//! The promise: while reload is on, a page shows its file. A save the page
//! cannot show changes nothing on screen and says why, with the file and
//! the line.
//!
//! The switch is the cargo feature `hot-reload`, off by default. With it
//! off every call above is empty: `open` hands the page as built,
//! `refresh` does nothing, and the host's code never reaches the
//! compiler, so the linker leaves the compiler out of the program. The
//! host writes the same code for both.
//!
//! What a reload cannot change, each refused with its line:
//! - the node list of a built page (a node added, removed or renamed):
//!   the host's code names nodes by numbers fixed at build (`NAMES`);
//! - a `/key`, a `res://` picture or an `atlas` line the app was not built
//!   with: the program has no string, no pixels and no slice for it.
//!
//! The pieces the `Reloader` is made of are public for a host with a loop
//! of its own: [`recompile`], [`same_shape`], [`swap`] and
//! `Runtime::carried_to`. They need no feature.

use std::cell::RefCell;

use crate::compile::{CompileOptions, Loader, compile_with};
use crate::diag::{Diagnostic, Severity, has_errors};
use crate::locale::Keys;
use crate::page::{AtlasDecl, AtlasShape, BehaviorSpec, ImageKey, ImageSlice, Images, Kind, Page, StaticPage};
use crate::runtime::Runtime;

/// Everything a reload needs to know about the build an app was made
/// from. `build_pages` writes one as the constant `BUILD`; a host hands
/// it whole to [`Reloader::new`].
///
/// Needed because a reloaded page must index the build's tables the way
/// the built pages do: a `VText` key, a picture id and an atlas slice id
/// are numbers into tables the running program already holds.
#[derive(Clone, Copy)]
pub struct BuildTables<'a> {
    /// The build's translation keys, `(name, English)` in id order.
    pub keys: &'a [(&'a str, &'a str)],
    /// The build's pictures in id order, as `IMAGES` holds them: the path
    /// a file names, and the file's bytes when the build embedded them.
    pub images: &'a [(&'a str, Option<&'a [u8]>)],
    /// The build's atlas slices, by `ImageKey::slice_index` (`SLICES`).
    pub slices: &'a [ImageSlice],
    /// Every `atlas` line of the build.
    pub atlases: &'a [BuiltAtlas<'a>],
    /// Node types the host draws from code — the same list the build
    /// compiled with (`CompileOptions::code_controls`).
    pub code_controls: &'a [&'a str],
    /// The host's own input behaviors (`CompileOptions::behaviors`).
    pub behaviors: &'a [BehaviorSpec],
    /// The build's `CompileOptions::require_keys`.
    pub require_keys: bool,
    /// Every page of the build with the file it came from.
    pub pages: &'a [BuiltPage<'a>],
}

impl BuildTables<'_> {
    /// A build with nothing in it: no key, no picture, no page.
    pub const EMPTY: BuildTables<'static> = BuildTables { keys: &[], images: &[], slices: &[], atlases: &[], code_controls: &[], behaviors: &[], require_keys: false, pages: &[] };
}

/// One page of a build: the file it was compiled from and what that file
/// said at build time.
#[derive(Clone, Copy)]
pub struct BuiltPage<'a> {
    /// The page's file as the `ui` folder names it (`effect_page.gogo`).
    /// Empty for a page gogo itself brings (the edit bar), which has no
    /// file to follow.
    pub file: &'a str,
    pub page: &'a StaticPage,
    /// The files its compile read through `uses`, as the files name them
    /// from the `ui` folder (`styles/theme.gogo`).
    pub uses: &'a [&'a str],
    /// The [`TextStamp`] of the file and of those files when the app was
    /// built. A reload compares it with the files as they are now, so a
    /// page whose files did not change is never compiled again.
    pub text: u64,
}

/// One `atlas` line of a build.
#[derive(Clone, Copy, Debug)]
pub struct BuiltAtlas<'a> {
    pub name: &'a str,
    /// What the line said, as a number (and what every other line of the
    /// build that declared the same atlas said). A reload takes an atlas
    /// as built, so its line must still say one of these.
    pub said: &'a [u64],
    pub shape: BuiltShape<'a>,
}

/// `page::AtlasShape` as a constant: the slices of one atlas.
#[derive(Clone, Copy, Debug)]
pub enum BuiltShape<'a> {
    /// `cell=WxH`: the cells in reading order.
    Grid(&'a [ImageKey]),
    /// Named slices, in the order written.
    Named(&'a [(&'a str, ImageKey)]),
    /// The whole picture.
    Whole(ImageKey),
}

/// A number for what a page's files say: the page's own text, then the
/// name and the text of every file it `uses`.
///
/// Needed so a reload knows, without compiling, whether a page's files
/// differ from what the page shows: the build writes the number into
/// `BUILD`, and the `Reloader` computes it again from the files. Line
/// endings do not count, so a checkout that turns `\n` into `\r\n` is not
/// a change.
pub struct TextStamp(u64);

impl TextStamp {
    /// A stamp over the page's own text.
    pub fn of(page_text: &str) -> TextStamp {
        let mut stamp = TextStamp(0xcbf2_9ce4_8422_2325);
        stamp.bytes(page_text.as_bytes());
        stamp
    }

    /// Adds one file the page `uses`: its name and its text, or `None`
    /// when the file is not there.
    pub fn used(&mut self, name: &str, text: Option<&str>) {
        self.mark(1);
        self.bytes(name.as_bytes());
        match text {
            Some(text) => {
                self.mark(2);
                self.bytes(text.as_bytes());
            }
            None => self.mark(3),
        }
    }

    pub fn finish(self) -> u64 {
        self.0
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            if *b != b'\r' {
                self.0 = (self.0 ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }

    /// A byte no text holds, between two parts.
    fn mark(&mut self, part: u8) {
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0000_0100_0000_01b3);
        self.0 = (self.0 ^ part as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
}

/// `recompile`, also handing back the files the compile read through
/// `uses`.
fn compile_against(source: &str, source_name: &str, loader: Loader<'_>, baked: &BuildTables<'_>) -> Result<(Page, Vec<String>), Vec<Diagnostic>> {
    let keys = RefCell::new(Keys::from_table(baked.keys));
    let mut images: Images = Images::from_names(baked.images.iter().map(|(name, _)| *name));
    images.slices = baked.slices.to_vec();
    images.atlases = baked
        .atlases
        .iter()
        .map(|a| {
            let shape: AtlasShape = match a.shape {
                BuiltShape::Grid(keys) => AtlasShape::Grid(keys.to_vec()),
                BuiltShape::Named(list) => AtlasShape::Named(list.iter().map(|(name, key)| (name.to_string(), *key)).collect()),
                BuiltShape::Whole(key) => AtlasShape::Whole(key),
            };
            AtlasDecl { name: a.name.to_string(), shape, origin: String::new(), said: a.said.to_vec() }
        })
        .collect();
    let images = RefCell::new(images);
    let options = CompileOptions { code_controls: baked.code_controls, behaviors: baked.behaviors, keys: Some(&keys), images: Some(&images), require_keys: baked.require_keys, source_name, refuse_new_keys_and_pictures: true, ..CompileOptions::default() };
    let compiled = compile_with(source, loader, options);
    if has_errors(&compiled.diags) {
        return Err(compiled.diags.into_iter().filter(|d| d.severity == Severity::Error).collect());
    }
    Ok((compiled.page, compiled.uses))
}

/// Compiles `source` (called `source_name` in diagnostics, e.g.
/// `effect_select.gogo`; `loader` reads its `uses` targets) so every key,
/// picture and atlas slice id in the result means what the built page's
/// does. `Err` = the errors, each at its line: the compiler's own, a
/// `/key` not in the build (G8004), a picture not in the build (G2021) or
/// an `atlas` line that is not the build's (G2022).
pub fn recompile(source: &str, source_name: &str, loader: Loader<'_>, baked: &BuildTables<'_>) -> Result<Page, Vec<Diagnostic>> {
    compile_against(source, source_name, loader, baked).map(|(page, _)| page)
}

/// Why two pages are not the same list of nodes (`same_shape`,
/// `Runtime::carried_to`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeChange {
    /// A node was added or removed.
    Count { old: usize, new: usize },
    /// The node at `index` has another kind or name.
    Node { index: usize, old_kind: Kind, old_name: String, new_kind: Kind, new_name: String },
    /// The row of `repeat` holds other nodes than the rows on screen.
    Row { repeat: String },
    /// `repeat` keys its rows another way than the rows on screen were
    /// keyed (another `by` field, or `by` added or removed).
    RowKey { repeat: String },
}

impl std::fmt::Display for ShapeChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeChange::Count { old, new } => write!(f, "node count changed ({old} -> {new}): adding or removing a node needs a rebuild"),
            ShapeChange::Node { index, old_kind, old_name, new_kind, new_name } => {
                write!(f, "node {index} changed ({old_kind:?} `{old_name}` -> {new_kind:?} `{new_name}`): a changed node list needs a rebuild")
            }
            ShapeChange::Row { repeat } => write!(f, "the row of repeat `{repeat}` holds other nodes than the rows on screen: a changed node list needs a rebuild"),
            ShapeChange::RowKey { repeat } => write!(f, "repeat `{repeat}` keys its rows another way than the rows on screen: a changed `by` needs a rebuild"),
        }
    }
}

/// The first node at which two pages differ in `from..to`, by kind or
/// name.
pub(crate) fn first_changed_node(old: &Page, new: &Page, from: usize, to: usize) -> Option<ShapeChange> {
    for index in from..to {
        let (a, b) = (&old.nodes[index], &new.nodes[index]);
        let (old_name, new_name) = (old.str(a.name), new.str(b.name));
        if a.kind != b.kind || old_name != new_name {
            return Some(ShapeChange::Node { index, old_kind: a.kind, old_name: old_name.to_string(), new_kind: b.kind, new_name: new_name.to_string() });
        }
    }
    None
}

/// Do two pages DECLARE the same node list — count, kinds and names in
/// order? A host's `NAMES` are indices into that list; a reload may change
/// anything else.
///
/// Only the nodes a page's file declared are compared. The rows a running
/// page added (`set_items`, `set_count`) are not the file's; they are
/// made again and checked when the state is carried over
/// (`Runtime::carried_to`).
///
/// Returns `Err` naming the first node that differs.
pub fn same_shape(old: &Page, new: &Page) -> Result<(), ShapeChange> {
    let (old_count, new_count) = (old.file_node_count(), new.file_node_count());
    if old_count != new_count {
        return Err(ShapeChange::Count { old: old_count, new: new_count });
    }
    match first_changed_node(old, new, 0, old_count) {
        Some(change) => Err(change),
        None => Ok(()),
    }
}

/// Why a saved file is not on screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Why {
    /// The file does not compile, or names a key, a picture or an atlas
    /// the app was not built with: each error at its line.
    Errors(Vec<Diagnostic>),
    /// The node list changed.
    Shape(ShapeChange),
    /// The rows on screen cannot be filled through the file's new row:
    /// the message `set_items` gives for the same items.
    Items(String),
}

/// A save a page could not show: the file, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Refused {
    /// The page's file, as the `ui` folder names it.
    pub file: String,
    pub why: Why,
}

impl std::fmt::Display for Refused {
    /// One line per problem, the file first: `effect_page.gogo:41 G0057
    /// <message>` for an error at a line, `effect_page.gogo: <reason>`
    /// for the rest.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.why {
            Why::Errors(errors) => {
                for (i, d) in errors.iter().enumerate() {
                    if i > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{}:{} {} {}", self.file, d.line, d.code, d.message)?;
                }
                Ok(())
            }
            Why::Shape(change) => write!(f, "{}: {change}", self.file),
            Why::Items(why) => write!(f, "{}: {why}", self.file),
        }
    }
}

/// Puts a freshly compiled `fresh` in the place of `page`, with the
/// runtime's state carried over: the screen stays where it was and only
/// what the file says changes.
///
/// On `Err` nothing changed: `page` and `runtime` are as they were.
pub fn swap(page: &mut Page, runtime: &mut Runtime, mut fresh: Page) -> Result<(), Why> {
    same_shape(page, &fresh).map_err(Why::Shape)?;
    let carried: Runtime = runtime.carried_to(page, &mut fresh)?;
    *runtime = carried;
    *page = fresh;
    Ok(())
}

/// Which file a page follows and which text of it the page shows.
/// Written by the [`Reloader`], and by `Page::from_static`, which notes
/// the built page a copy was made from; a page compiled at run time
/// follows no file until it is handed over (`Reloader::follow`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Follows {
    /// Where the built page this copy was made from sits in the program
    /// (`Page::from_static`); 0 = not made from one. The reloader finds
    /// the page's file by it at the page's first `refresh`.
    #[cfg_attr(not(feature = "hot-reload"), allow(dead_code))]
    built_at: usize,
    /// The file's place in the reloader's list, plus one; 0 = no file.
    #[cfg_attr(not(feature = "hot-reload"), allow(dead_code))]
    file: u32,
    /// The [`TextStamp`] of the text the page shows.
    #[cfg_attr(not(feature = "hot-reload"), allow(dead_code))]
    text: u64,
    /// The reloader's change count when the page was last compared with
    /// its file.
    #[cfg_attr(not(feature = "hot-reload"), allow(dead_code))]
    seen: u32,
    /// The last save of the file could not be shown.
    #[cfg_attr(not(feature = "hot-reload"), allow(dead_code))]
    is_refused: bool,
}

impl Follows {
    /// For a copy of the built page `built`.
    pub(crate) fn of_built(built: &StaticPage) -> Follows {
        Follows { built_at: built as *const StaticPage as usize, ..Follows::default() }
    }
}

/// Where a reload reads `.gogo` text from.
#[derive(Clone)]
pub struct Sources {
    #[cfg(feature = "hot-reload")]
    kind: live::Kind,
}

impl Sources {
    /// A folder of `.gogo` files: on a PC the project's own `ui` folder
    /// (saving in the editor is all it takes), on a phone a folder a
    /// script copies the files into. The reloader watches the folder's
    /// file times on a thread of its own.
    #[inline(always)]
    pub fn dir(path: impl Into<std::path::PathBuf>) -> Sources {
        #[cfg(feature = "hot-reload")]
        {
            Sources { kind: live::Kind::Dir(path.into()) }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = path;
            Sources {}
        }
    }

    /// A function that reads one file by the name a page writes it
    /// (`settings.gogo`, `styles/theme.gogo`); `None` = no such file. For
    /// text that comes over a socket or from an editor. Nothing is
    /// watched: the host calls [`Reloader::mark_changed`] when text
    /// arrived.
    #[inline(always)]
    pub fn read_with(read: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Sources {
        #[cfg(feature = "hot-reload")]
        {
            Sources { kind: live::Kind::Read(std::sync::Arc::new(read)) }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = read;
            Sources {}
        }
    }
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "hot-reload")]
        {
            match &self.kind {
                live::Kind::Dir(dir) => write!(f, "Sources::dir({})", dir.display()),
                live::Kind::Read(_) => write!(f, "Sources::read_with(..)"),
            }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            write!(f, "Sources (hot reload is not compiled in)")
        }
    }
}

/// Keeps pages in step with their files. See the module's own words for
/// the three calls; every call is empty without the `hot-reload` feature.
pub struct Reloader {
    #[cfg(feature = "hot-reload")]
    live: Option<Box<live::Live>>,
}

impl std::fmt::Debug for Reloader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Reloader({})", if self.is_on() { "on" } else { "off" })
    }
}

impl Reloader {
    /// A reloader that follows no file: `open` hands the page as built
    /// and `refresh` does nothing. For a run that must not see a page
    /// change under it (a test, a store build that keeps the feature on).
    #[inline(always)]
    pub fn off() -> Reloader {
        Reloader {
            #[cfg(feature = "hot-reload")]
            live: None,
        }
    }

    /// A reloader for an app built with `build_pages`: `build` is the
    /// generated `BUILD`, `sources` where the page files are read from
    /// while the app runs.
    #[inline(always)]
    pub fn new(build: &'static BuildTables<'static>, sources: Sources) -> Reloader {
        #[cfg(feature = "hot-reload")]
        {
            Reloader { live: Some(Box::new(live::Live::new(Some(build), sources, live::HostWords::default()))) }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = (build, sources);
            Reloader {}
        }
    }

    /// A reloader for an app that compiles its pages when it starts and
    /// finds its nodes by name (`Page::find`): no build, so no number is
    /// fixed. Such a page is handed over with [`Reloader::follow`], and a
    /// reload may change anything in it.
    #[inline(always)]
    pub fn files(sources: Sources) -> Reloader {
        #[cfg(feature = "hot-reload")]
        {
            Reloader { live: Some(Box::new(live::Live::new(None, sources, live::HostWords::default()))) }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = sources;
            Reloader {}
        }
    }

    /// As [`Reloader::files`], for an app that compiles its pages with
    /// options of its own: every reload compiles a followed file with
    /// the code controls, the behaviors and the `require_keys` of
    /// `options`, the options the app gives `compile_with` at start.
    ///
    /// Needed because a file that names a host's code control or
    /// behavior compiles only when the compiler is told of it: a
    /// reloader that was not told refuses the first save of such a page
    /// ("unknown node type").
    ///
    /// The key table, the picture table and the asset reader of
    /// `options` are not kept: they are lent for one compile. So a
    /// page with an `atlas` line still needs a build to reload.
    #[inline(always)]
    pub fn files_with(sources: Sources, options: CompileOptions<'_>) -> Reloader {
        #[cfg(feature = "hot-reload")]
        {
            Reloader { live: Some(Box::new(live::Live::new(None, sources, live::HostWords::of(&options)))) }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = (sources, options);
            Reloader {}
        }
    }

    /// Whether pages follow their files: the feature is compiled in and
    /// this reloader was not made by `off`.
    #[inline(always)]
    pub fn is_on(&self) -> bool {
        #[cfg(feature = "hot-reload")]
        {
            self.live.is_some()
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            false
        }
    }

    /// `wake` is called, on the reloader's thread, when a file in the
    /// watched folder changed.
    ///
    /// Needed because a host that sleeps while nothing moves draws no
    /// frame, so nothing would call `refresh`: `wake` asks the host's
    /// event loop for one.
    #[inline(always)]
    pub fn on_change(&mut self, wake: impl Fn() + Send + 'static) {
        #[cfg(feature = "hot-reload")]
        {
            if let Some(live) = &self.live {
                live.set_wake(Box::new(wake));
            }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = wake;
        }
    }

    /// Tells the reloader that page text may have changed. A host with
    /// sources of its own (`Sources::read_with`) calls it when text
    /// arrived; a watched folder marks its own changes.
    #[inline(always)]
    pub fn mark_changed(&self) {
        #[cfg(feature = "hot-reload")]
        {
            if let Some(live) = &self.live {
                live.mark_changed();
            }
        }
    }

    /// Whether a change was marked that `take_changed` has not handed out
    /// yet. A look, not a take: a host that parks its thread asks this
    /// between sleeps.
    #[inline(always)]
    pub fn has_pending_change(&self) -> bool {
        #[cfg(feature = "hot-reload")]
        {
            self.live.as_ref().is_some_and(|live| live.has_pending_change())
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            false
        }
    }

    /// Whether a change was marked since the last call: true once per
    /// change. For a host that refreshes its pages in one place when
    /// something changed; a host that calls `refresh` every frame does
    /// not need it.
    #[inline(always)]
    pub fn take_changed(&mut self) -> bool {
        #[cfg(feature = "hot-reload")]
        {
            self.live.as_mut().is_some_and(|live| live.take_changed())
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            false
        }
    }

    /// The page `built` as its file says NOW, when that file is in the
    /// sources, compiles and declares the nodes the app was built with;
    /// else the page as built. A page opened this way follows its file
    /// (`refresh`).
    ///
    /// Needed so a page opened after an edit shows the edit, not the page
    /// of the last build. When the file cannot be shown the page as built
    /// is returned, and the page's next `refresh` returns why.
    ///
    /// A host that makes its pages with `Page::from_static` may leave
    /// them so: `refresh` takes such a page over the first time it sees
    /// it, and brings it in step with its file then.
    #[inline(always)]
    pub fn open(&mut self, built: &'static StaticPage) -> Page {
        #[cfg(feature = "hot-reload")]
        {
            if let Some(live) = &mut self.live {
                return live.open(built);
            }
        }
        Page::from_static(built)
    }

    /// Hands over a page the host compiled itself from `file` (as the
    /// sources name it): from now on `refresh` keeps it in step with that
    /// file. For a reloader made by [`Reloader::files`].
    #[inline(always)]
    pub fn follow(&mut self, page: &mut Page, file: &str) {
        #[cfg(feature = "hot-reload")]
        {
            if let Some(live) = &mut self.live {
                live.follow(page, file);
            }
        }
        #[cfg(not(feature = "hot-reload"))]
        {
            let _ = (page, file);
        }
    }

    /// Brings `page` in step with its file, when the file (or a file it
    /// `uses`) says something else than the page shows: compiles it,
    /// checks it against the build, carries the runtime's state over and
    /// swaps the page in. When nothing changed since the last call it
    /// costs one number compare, so a host may call it every frame.
    ///
    /// A page of a built app keeps its node list (the code's `NAMES` are
    /// numbers into it); a page handed over with `follow` may change in
    /// any way, and starts fresh when its node list changed — the host
    /// then writes again what its code writes.
    ///
    /// Returns `Ok(true)` when the page shows its file again after this
    /// call (a new page is in, or the file went back to what the page
    /// shows after a refused save): the host drops what it kept about the
    /// page's layout and hides an error it showed. `Ok(false)` = nothing
    /// to do. `Err` = the save cannot be shown; nothing changed, and the
    /// same save is not reported again until another file changes.
    #[inline(always)]
    pub fn refresh(&mut self, page: &mut Page, runtime: &mut Runtime) -> Result<bool, Refused> {
        #[cfg(feature = "hot-reload")]
        {
            if let Some(live) = &mut self.live {
                return live.refresh(page, runtime);
            }
        }
        let _ = (page, runtime);
        Ok(false)
    }
}

#[cfg(feature = "hot-reload")]
mod live {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime};

    use super::{BuildTables, Follows, Refused, Sources, TextStamp, Why, compile_against, same_shape, swap};
    use crate::compile::{CompileOptions, compile_with};
    use crate::diag::{Severity, has_errors};
    use crate::page::{BehaviorSpec, Page, StaticPage};
    use crate::runtime::Runtime;

    /// How often a watched folder's file times are read.
    const POLL: Duration = Duration::from_millis(250);

    /// How long after its save a file's text is compared too.
    const FRESH: Duration = Duration::from_secs(2);

    /// The watching thread's name. It is also how a store build is
    /// checked: this text is in a program only when hot reload was
    /// compiled in.
    const WATCH_THREAD: &str = "gogo-reload-watch";

    #[derive(Clone)]
    pub(super) enum Kind {
        Dir(PathBuf),
        Read(Arc<dyn Fn(&str) -> Option<String> + Send + Sync>),
    }

    impl Sources {
        fn read(&self, rel: &str) -> Option<String> {
            match &self.kind {
                Kind::Dir(dir) => std::fs::read_to_string(dir.join(rel)).ok(),
                Kind::Read(read) => read(rel),
            }
        }
    }

    /// What the watching thread and the reloader share.
    struct Changes {
        /// Goes up by one for every change noticed or marked.
        count: AtomicU32,
        /// Set when the reloader is dropped: the thread ends.
        stop: AtomicBool,
        wake: Mutex<Option<Box<dyn Fn() + Send>>>,
    }

    /// What a look at one file kept: its save time and length, and a
    /// number for its text while the save is fresh.
    ///
    /// Needed because a save time moves in steps of the system clock (up
    /// to 16 ms on Windows): two saves of the same length inside one step
    /// leave the time and the length as they were. A file saved in the
    /// last `FRESH` is read, so the second save is seen by its text; an
    /// older file is only asked for its time, which costs no read.
    #[derive(Clone, Copy)]
    struct Seen {
        saved: SystemTime,
        len: u64,
        text: Option<u64>,
    }

    /// Every `.gogo` file under a folder, as the last look found it.
    struct DirWatch {
        dir: PathBuf,
        seen: BTreeMap<PathBuf, Seen>,
    }

    impl DirWatch {
        /// A watch that already knows the folder as it is now, so what is
        /// there at the start is not a change.
        fn primed(dir: PathBuf) -> DirWatch {
            let mut seen = BTreeMap::new();
            look(&dir, &mut seen);
            DirWatch { dir, seen }
        }

        /// Whether a `.gogo` file appeared, vanished or was saved since
        /// the last look. A folder that is not there reads as empty.
        fn has_changed(&mut self) -> bool {
            let mut now = BTreeMap::new();
            look(&self.dir, &mut now);
            let changed: bool = now.len() != self.seen.len()
                || now.iter().any(|(path, new)| match self.seen.get(path) {
                    None => true,
                    Some(old) => old.saved != new.saved || old.len != new.len || matches!((old.text, new.text), (Some(a), Some(b)) if a != b),
                });
            self.seen = now;
            changed
        }
    }

    fn look(dir: &Path, out: &mut BTreeMap<PathBuf, Seen>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.is_dir() {
                look(&path, out);
            } else if path.extension().is_some_and(|e| e == "gogo") {
                let Ok(meta) = entry.metadata() else { continue };
                let Ok(saved) = meta.modified() else { continue };
                // A save time ahead of the clock counts by its distance
                // too: a folder copied from another machine may be hours
                // ahead, and is then not read on every look.
                let is_fresh: bool = match saved.elapsed() {
                    Ok(age) => age < FRESH,
                    Err(ahead) => ahead.duration() < FRESH,
                };
                let text: Option<u64> = if is_fresh { std::fs::read(&path).ok().map(|bytes| bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3))) } else { None };
                out.insert(path, Seen { saved, len: meta.len(), text });
            }
        }
    }

    /// One page file the reloader keeps pages in step with.
    struct File {
        /// As the sources name it (`effect_page.gogo`).
        name: String,
        /// The files its last compile read through `uses`.
        uses: Vec<String>,
        /// The page as built with the stamp of its text at build time;
        /// `None` for a page the host compiled itself (`follow`).
        built: Option<(&'static StaticPage, u64)>,
        /// The last compile of the file, by the stamp of the text it
        /// read. Every page that follows the file takes its copy from
        /// here, so one save is compiled once.
        latest: Option<(u64, Result<Page, Why>)>,
    }

    /// What an app that compiles its own pages told the compiler, in
    /// lists the reloader owns.
    ///
    /// Needed because `CompileOptions` borrows its lists for one compile,
    /// and the reloader compiles again at every save.
    #[derive(Default)]
    pub(super) struct HostWords {
        code_controls: Vec<String>,
        behaviors: Vec<BehaviorSpec>,
        require_keys: bool,
    }

    impl HostWords {
        pub(super) fn of(options: &CompileOptions<'_>) -> HostWords {
            HostWords { code_controls: options.code_controls.iter().map(|c| c.to_string()).collect(), behaviors: options.behaviors.to_vec(), require_keys: options.require_keys }
        }
    }

    pub(super) struct Live {
        build: Option<&'static BuildTables<'static>>,
        /// The options of an app with no build (`Reloader::files_with`).
        host: HostWords,
        sources: Sources,
        changes: Arc<Changes>,
        /// The change count `take_changed` last handed out.
        taken: u32,
        files: Vec<File>,
    }

    impl Drop for Live {
        fn drop(&mut self) {
            self.changes.stop.store(true, Ordering::Release);
        }
    }

    impl Live {
        pub(super) fn new(build: Option<&'static BuildTables<'static>>, sources: Sources, host: HostWords) -> Live {
            let changes = Arc::new(Changes { count: AtomicU32::new(1), stop: AtomicBool::new(false), wake: Mutex::new(None) });
            if let Kind::Dir(dir) = &sources.kind {
                let mut watch: DirWatch = DirWatch::primed(dir.clone());
                let shared: Arc<Changes> = changes.clone();
                // A thread that cannot start leaves a reloader that
                // follows marked changes only (`mark_changed`).
                let _ = std::thread::Builder::new().name(WATCH_THREAD.into()).spawn(move || {
                    loop {
                        std::thread::sleep(POLL);
                        if shared.stop.load(Ordering::Acquire) {
                            break;
                        }
                        if watch.has_changed() {
                            shared.count.fetch_add(1, Ordering::AcqRel);
                            if let Some(wake) = shared.wake.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                                wake();
                            }
                        }
                    }
                });
            }
            let files: Vec<File> = build.map_or(Vec::new(), |b| b.pages.iter().map(|p| File { name: p.file.to_string(), uses: p.uses.iter().map(|u| u.to_string()).collect(), built: Some((p.page, p.text)), latest: None }).collect());
            Live { build, host, sources, changes, taken: 1, files }
        }

        pub(super) fn set_wake(&self, wake: Box<dyn Fn() + Send>) {
            *self.changes.wake.lock().unwrap_or_else(|e| e.into_inner()) = Some(wake);
        }

        pub(super) fn mark_changed(&self) {
            self.changes.count.fetch_add(1, Ordering::AcqRel);
        }

        fn count(&self) -> u32 {
            self.changes.count.load(Ordering::Acquire)
        }

        pub(super) fn has_pending_change(&self) -> bool {
            self.count() != self.taken
        }

        pub(super) fn take_changed(&mut self) -> bool {
            let count: u32 = self.count();
            let changed: bool = count != self.taken;
            self.taken = count;
            changed
        }

        /// The stamp of what file `at` and the files it `uses` say now;
        /// `None` when the page's own file is not in the sources.
        fn stamp_now(&self, at: usize) -> Option<u64> {
            let file: &File = &self.files[at];
            if file.name.is_empty() {
                return None;
            }
            let text: String = self.sources.read(&file.name)?;
            let mut stamp = TextStamp::of(&text);
            for used in &file.uses {
                stamp.used(used, self.sources.read(used).as_deref());
            }
            Some(stamp.finish())
        }

        /// Compiles file `at` unless its last compile read the same text
        /// (`now`). Returns the stamp the compile stands for: `now`, or
        /// another number when the file's `uses` lines changed.
        fn compile(&mut self, at: usize, now: u64) -> u64 {
            if let Some((stamp, _)) = &self.files[at].latest {
                if *stamp == now {
                    return now;
                }
            }
            let name: String = self.files[at].name.clone();
            let Some(text) = self.sources.read(&name) else { return now };
            let sources: Sources = self.sources.clone();
            let loader = move |rel: &str| sources.read(rel);
            let (result, uses): (Result<Page, Why>, Option<Vec<String>>) = match (self.build, self.files[at].built.is_some()) {
                (Some(build), true) => match compile_against(&text, &name, &loader, build) {
                    Ok((page, uses)) => (Ok(page), Some(uses)),
                    Err(errors) => (Err(Why::Errors(errors)), None),
                },
                // A page the host compiled itself: no build to index, so
                // the file may name any key and any picture, and it is
                // compiled with the words the host told of.
                _ => {
                    let code_controls: Vec<&str> = self.host.code_controls.iter().map(String::as_str).collect();
                    let options = CompileOptions { source_name: &name, code_controls: &code_controls, behaviors: &self.host.behaviors, require_keys: self.host.require_keys, ..CompileOptions::default() };
                    let compiled = compile_with(&text, &loader, options);
                    if has_errors(&compiled.diags) {
                        (Err(Why::Errors(compiled.diags.into_iter().filter(|d| d.severity == Severity::Error).collect())), Some(compiled.uses))
                    } else {
                        (Ok(compiled.page), Some(compiled.uses))
                    }
                }
            };
            if let Some(uses) = uses {
                self.files[at].uses = uses;
            }
            // The stamp over the `uses` list this compile found, which is
            // what the next look compares.
            let stamp: u64 = self.stamp_now(at).unwrap_or(now);
            self.files[at].latest = Some((stamp, result));
            stamp
        }

        pub(super) fn open(&mut self, built: &'static StaticPage) -> Page {
            let mut page: Page = Page::from_static(built);
            let Some(at) = self.files.iter().position(|f| f.built.is_some_and(|(p, _)| std::ptr::eq(p, built))) else { return page };
            let as_built: u64 = self.files[at].built.map_or(0, |(_, text)| text);
            let count: u32 = self.count();
            let file: u32 = at as u32 + 1;
            page.follows = Follows { file, text: as_built, seen: count, ..page.follows };
            let Some(now) = self.stamp_now(at) else { return page };
            if now == as_built {
                return page;
            }
            let stamp: u64 = self.compile(at, now);
            if let Some((_, Ok(fresh))) = &self.files[at].latest {
                if same_shape(&page, fresh).is_ok() {
                    let mut fresh: Page = fresh.clone();
                    fresh.follows = Follows { file, text: stamp, seen: count, ..page.follows };
                    return fresh;
                }
            }
            // The file cannot be shown: the page as built, and its next
            // `refresh` says why.
            page.follows.seen = count.wrapping_sub(1);
            page
        }

        pub(super) fn follow(&mut self, page: &mut Page, name: &str) {
            let at: usize = match self.files.iter().position(|f| f.name == name) {
                Some(at) => at,
                None => {
                    self.files.push(File { name: name.to_string(), uses: Vec::new(), built: None, latest: None });
                    self.files.len() - 1
                }
            };
            // One compile of our own tells which files the page `uses`,
            // so a save of one of them is seen.
            let stamp: u64 = match self.stamp_now(at) {
                Some(now) => self.compile(at, now),
                None => 0,
            };
            page.follows = Follows { file: at as u32 + 1, text: stamp, seen: self.count(), ..Follows::default() };
        }

        pub(super) fn refresh(&mut self, page: &mut Page, runtime: &mut Runtime) -> Result<bool, Refused> {
            let mut follows: Follows = page.follows;
            if follows.file == 0 {
                // A copy of a built page nobody opened through `open`:
                // found by the built page it was made from, and compared
                // with its file right now.
                let found: Option<usize> = if follows.built_at == 0 { None } else { self.files.iter().position(|f| f.built.is_some_and(|(p, _)| p as *const StaticPage as usize == follows.built_at)) };
                let Some(at) = found else {
                    page.follows.built_at = 0;
                    return Ok(false);
                };
                follows.file = at as u32 + 1;
                follows.text = self.files[at].built.map_or(0, |(_, text)| text);
                follows.seen = 0;
                page.follows = follows;
            }
            let count: u32 = self.count();
            if follows.seen == count {
                return Ok(false);
            }
            page.follows.seen = count;
            let at: usize = follows.file as usize - 1;
            // A file that is gone: the page keeps what it shows.
            let Some(now) = self.stamp_now(at) else { return Ok(false) };
            if now == follows.text {
                page.follows.is_refused = false;
                return Ok(follows.is_refused);
            }
            let stamp: u64 = self.compile(at, now);
            let name: String = self.files[at].name.clone();
            let is_built: bool = self.files[at].built.is_some();
            let fresh: Page = match &self.files[at].latest {
                Some((_, Ok(fresh))) => fresh.clone(),
                Some((_, Err(why))) => {
                    page.follows.is_refused = true;
                    return Err(Refused { file: name, why: why.clone() });
                }
                None => return Ok(false),
            };
            let swapped: Result<(), Why> = if is_built || same_shape(page, &fresh).is_ok() {
                swap(page, runtime, fresh)
            } else {
                // A page the host compiled itself, with another node
                // list: no number is fixed, so it starts fresh.
                *runtime = Runtime::new(&fresh);
                *page = fresh;
                Ok(())
            };
            match swapped {
                Ok(()) => {
                    page.follows = Follows { file: follows.file, text: stamp, seen: count, is_refused: false, built_at: follows.built_at };
                    Ok(true)
                }
                Err(why) => {
                    page.follows.is_refused = true;
                    Err(Refused { file: name, why })
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A folder's files as they are at the start are not a change; a
        /// saved, a new and a removed `.gogo` file each are, once; a file
        /// of another kind is not.
        #[test]
        fn a_watched_folder_reports_each_change_once() {
            let dir: PathBuf = std::env::temp_dir().join(format!("gogo_reload_watch_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("styles")).unwrap();
            std::fs::write(dir.join("a.gogo"), "View a { }").unwrap();
            let mut watch = DirWatch::primed(dir.clone());
            assert!(!watch.has_changed(), "what is there at the start is not a change");
            std::fs::write(dir.join("a.gogo"), "View a { Label b text=\"x\" }").unwrap();
            assert!(watch.has_changed(), "a saved file");
            assert!(!watch.has_changed(), "reported once");
            // The same length, a moment later: the save time may not have
            // moved, so the text of a fresh file is compared too.
            std::fs::write(dir.join("a.gogo"), "View a { Label b text=\"y\" }").unwrap();
            assert!(watch.has_changed(), "a second save of the same length");
            assert!(!watch.has_changed(), "reported once");
            std::fs::write(dir.join("styles").join("theme.gogo"), "$accent = red\n").unwrap();
            assert!(watch.has_changed(), "a new file in a folder below");
            std::fs::write(dir.join("notes.txt"), "not a page").unwrap();
            assert!(!watch.has_changed(), "a file of another kind");
            std::fs::remove_file(dir.join("a.gogo")).unwrap();
            assert!(watch.has_changed(), "a removed file");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
