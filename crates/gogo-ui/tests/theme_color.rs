//! `Runtime::theme_color` and the generated `enum Theme`: code reads the
//! file's `$name` colors by a typed handle, following the active theme
//! variant (gogo-spec.md "Theme").
use gogo::emit::emit_rust;
use gogo::page::{Color_Straight_Srgb_4xU8_RGBA, StrId};
use gogo::{Runtime, ThemeName, compile};

const SRC: &str = "$accent = #E51400FF\n$gap = 4\n$link = $accent\ntheme dark {\n    $accent = #00FF00FF\n}\nView v { Rectangle swatch color=$accent }\n";

/// What the generated `Theme` variant does: carry the name's string id.
#[derive(Clone, Copy)]
struct Name(StrId);

impl ThemeName for Name {
    fn id(self) -> StrId {
        self.0
    }
}

#[test]
fn theme_color_follows_the_variant_and_refs() {
    let c = compile(SRC, &|_| None);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    let page = c.page;
    let accent = Name(page.intern_lookup("accent"));
    let link = Name(page.intern_lookup("link"));
    let gap = Name(page.intern_lookup("gap"));
    let mut rt = Runtime::new(&page);
    assert_eq!(rt.theme_color(&page, accent), Some(Color_Straight_Srgb_4xU8_RGBA([0xE5, 0x14, 0x00, 0xFF])));
    assert_eq!(rt.theme_color(&page, link), rt.theme_color(&page, accent), "a `$ref` is followed");
    assert_eq!(rt.theme_color(&page, gap), None, "a number isn't a color");
    rt.variant = page.intern_lookup("dark");
    assert_eq!(rt.theme_color(&page, accent), Some(Color_Straight_Srgb_4xU8_RGBA([0x00, 0xFF, 0x00, 0xFF])), "the dark block's value");
    assert_eq!(rt.theme_color(&page, link), rt.theme_color(&page, accent));
}

/// The build writes one variant per theme name, its discriminant the
/// name's string id.
#[test]
fn the_build_emits_a_theme_enum() {
    let c = compile(SRC, &|_| None);
    let src: String = emit_rust(&c.page, "t").unwrap();
    let accent: usize = c.page.intern_lookup("accent").index();
    assert!(src.contains("pub enum Theme {"), "{src}");
    assert!(src.contains(&format!("        Accent = {accent},")), "{src}");
    assert_eq!(src.matches("        Accent = ").count(), 1, "the dark block's entry is the same name");
    assert!(src.contains("        Gap = ") && src.contains("        Link = "), "{src}");
    assert!(src.contains("impl gogo::ThemeName for Theme"), "{src}");
    let bare = compile("View v { Panel p width=10 }", &|_| None);
    assert!(!emit_rust(&bare.page, "t").unwrap().contains("enum Theme"), "no theme values, no enum");
}
