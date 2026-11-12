//! What starting a built page costs: one allocation for each table, and
//! none for its text.

mod reload_kit;

use gogo_ui::runtime::Runtime;
use gogo_ui::{NodeId, Page, StaticPage, StrId, compile};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// The allocations this thread made since `allocations` began; `None`
    /// outside a count.
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}

/// The system allocator, counting the allocations of a thread that asked.
///
/// Needed because "no string is copied" is a statement about allocations,
/// and nothing else in a test can see one.
struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNT.try_with(|c| {
            if let Some(n) = c.get() {
                c.set(Some(n + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// How many allocations `work` makes on this thread.
fn allocations(work: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(Some(0)));
    work();
    COUNT.with(|c| c.replace(None)).expect("the count was started above")
}

/// A page of 200 labels, as a build emits it.
fn built() -> &'static StaticPage {
    let mut src = String::from("View v {\n Column c {\n");
    for i in 0..200 {
        src.push_str(&format!("  Label l{i} text=\"Line {i}\"\n"));
    }
    src.push_str(" }\n}\n");
    let compiled = compile(&src, &|_| None);
    assert!(!gogo_ui::has_errors(&compiled.diags), "{:#?}", compiled.diags);
    reload_kit::as_built(&compiled.page, &[])
}

/// A page started from built tables reads its text where the build put
/// it: no string is copied, so the start allocates once for each table
/// and not once more for each string.
#[test]
fn starting_a_built_page_copies_no_text() {
    let built: &'static StaticPage = built();
    assert!(built.strings.len() > 400, "the page holds a name and a text for each label: {}", built.strings.len());

    let mut started: Option<Page> = None;
    let count: usize = allocations(|| started = Some(Page::from_static(built)));
    let page: Page = started.expect("the page was started above");
    assert!(count < 40, "a start allocates for its tables alone: {count} allocations for {} strings", built.strings.len());

    let id: StrId = page.intern_lookup("Line 7");
    assert_eq!(page.str(id), "Line 7");
    assert!(std::ptr::eq(page.str(id).as_ptr(), built.strings[id.index()].as_ptr()), "the page reads the build's own text");
}

/// A text that code writes is the page's own: it shows, a later write
/// takes its place, and the build's text is never written to.
#[test]
fn a_text_written_from_code_is_the_pages_own() {
    let built: &'static StaticPage = built();
    let mut page: Page = Page::from_static(built);
    let mut rt: Runtime = Runtime::new(&page);
    let label: NodeId = page.find("l7").expect("label l7");
    let id: StrId = page.intern_lookup("Line 7");

    rt.set_text(&mut page, label, "changed");
    assert_eq!(rt.text_of(&page, label), Some("changed"));
    rt.set_text(&mut page, label, "changed again");
    assert_eq!(rt.text_of(&page, label), Some("changed again"));
    assert_eq!(built.strings[id.index()], "Line 7");
    assert_eq!(page.str(id), "Line 7", "the text the file wrote stays for the other nodes that show it");

    // A word the page did not hold is added to the page, not to the build.
    let before: usize = built.strings.len();
    let added: StrId = page.intern("a new word");
    assert_eq!(page.str(added), "a new word");
    assert_eq!(built.strings.len(), before);
}
