//! `release=unused` (DECISIONS.md D98; user call 2026-09-28): a picture
//! the app may free while no live page names it — the sticker sheets. The
//! word goes where the picture is declared (an `atlas` line, an Image that
//! names a file), and the picture is freed only when EVERY declaring line
//! says it: one line without it keeps the picture for the session, the
//! way one eager line makes a picture eager everywhere.
use std::collections::HashMap;

use gogo::page::ImageKey;
use gogo::{Compiled, CompileOptions, compile_with};

/// A PNG's first 24 bytes: all the atlas reads.
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    b.extend_from_slice(b"IHDR");
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b
}

fn build(src: &str) -> Compiled {
    let mut f: HashMap<&str, Vec<u8>> = HashMap::new();
    f.insert("res://s/stars.jpg", png(480, 720));
    f.insert("res://s/big.png", png(64, 64));
    let read = |p: &str| f.get(p).cloned().ok_or_else(|| "no such file".to_string());
    compile_with(src, &|_| None, CompileOptions { assets: Some(&read), ..CompileOptions::default() })
}

fn ok(src: &str) -> Compiled {
    let c = build(src);
    assert!(!gogo::has_errors(&c.diags), "{:#?}", c.diags);
    c
}

fn codes(src: &str) -> Vec<String> {
    build(src).diags.iter().map(|d| d.code.clone()).collect()
}

fn key(c: &Compiled, path: &str) -> ImageKey {
    c.images.get(path).expect("the picture is in the image table")
}

#[test]
fn an_atlas_line_and_an_image_line_mark_their_pictures() {
    let c = ok("atlas stars source=\"res://s/stars.jpg\", cell=80x80, release=unused\nView v { Column c { Image a 20x20, source=stars[3]\n Image b 20x20, source=\"res://s/big.png\", release=unused } }");
    assert_eq!(c.images.release_unused(), vec![key(&c, "res://s/stars.jpg"), key(&c, "res://s/big.png")], "both declaring lines said it; an Image showing a slice declares nothing");
}

#[test]
fn one_declaring_line_without_the_word_keeps_the_picture() {
    let c = ok("View v { Column c { Image a 20x20, source=\"res://s/big.png\", release=unused\n Image b 20x20, source=\"res://s/big.png\" } }");
    assert!(c.images.release_unused().is_empty(), "the second Image keeps it for the session");
    let c = ok("atlas stars source=\"res://s/stars.jpg\", cell=80x80, release=unused\nView v { Image a 20x20, source=\"res://s/stars.jpg\" }");
    assert!(c.images.release_unused().is_empty(), "an Image naming the atlas's picture file directly keeps it");
    let c = ok("View v { Image a 20x20, source=\"res://s/big.png\" }");
    assert!(c.images.release_unused().is_empty(), "no word, no release: the default keeps every picture");
}

#[test]
fn release_errors() {
    assert_eq!(codes("atlas stars source=\"res://s/stars.jpg\", cell=80x80, release=gone"), vec!["G2020"], "one word only");
    assert_eq!(codes("atlas stars source=\"res://s/stars.jpg\", cell=80x80\nView v { Image a 20x20, source=stars[1], release=unused }"), vec!["G2020"], "a slice's picture is declared by its atlas line");
    assert_eq!(codes("View v { Image a 20x20, source=\"collage://preview\", release=unused }"), vec!["G2020"], "a picture the app draws itself has no file to decode again");
    assert!(!codes("View v { Image a 20x20, source=\"res://s/big.png\", release=never }").is_empty(), "`unused` is the only value");
    assert!(!codes("View v { Rectangle r 20x20, release=unused }").is_empty(), "an Image word");
}
