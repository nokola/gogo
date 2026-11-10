//! The one image registry (2026-09-05, user): a runtime table seeded from
//! the build (`Images::from_names`) is SEALED for `res://` files — interning
//! one the build did not embed is the `unembedded_image` lint (a debug
//! panic naming the path), while host pictures (`face://3`) stay welcome
//! and a build-time table interns anything.
use gogo_ui::{Images, NO_IMAGE};

#[test]
fn a_sealed_table_takes_host_pictures_and_known_files() {
    let mut images = Images::from_names(["res://assets/icons/eraser.png"]);
    let eraser = images.get("res://assets/icons/eraser.png").expect("seeded");
    assert_eq!(images.intern("res://assets/icons/eraser.png"), eraser, "a known file is a lookup");
    let face = images.intern("face://3");
    assert_ne!(face, eraser);
    assert_ne!(face, NO_IMAGE);
    assert_eq!(images.name(face), "face://3");
}

#[test]
#[should_panic(expected = "unembedded_image")]
fn a_sealed_table_refuses_an_unembedded_file() {
    let mut images = Images::from_names(["res://assets/icons/eraser.png"]);
    let _ = images.intern("res://assets/icons/not-embedded.png");
}

#[test]
fn a_build_time_table_interns_anything() {
    let mut images = Images::default();
    let a = images.intern("res://assets/icons/a.png");
    let b = images.intern("res://assets/icons/b.png");
    assert_ne!(a, b);
    assert_eq!(images.len(), 2);
}
