//! gogo-text's isolation rule as a red-check: it names no GPU, window or
//! phone crate.
//!
//! Needed because the text engine has to serve every renderer and every
//! host (it hands out glyph rectangles and two atlases of bytes, nothing
//! more), and a dependency on a GPU or window crate builds fine on the PC
//! it is first tried on, so nothing else goes red when that promise
//! breaks.
//!
//! Unlike gogo-ui, this crate reads the disk on purpose: the OS font scan
//! and a fallback font file are part of its job.

use std::fs;
use std::path::Path;

/// Dependency names this crate must never list (prefix match: `android`
/// covers `android-activity` and `android_logger`).
const OS_OR_GPU: &[&str] = &["wgpu", "winit", "android", "jni", "image", "egui", "pollster", "env_logger", "raw-window-handle", "ndk"];

/// Every dependency name in a manifest's text, in order: the keys of each
/// `[...dependencies]` table, and the name in each
/// `[...dependencies.NAME]` header.
///
/// Needed as a plain line scan, with no toml crate, so that this test adds
/// no dependency of its own to the crate it guards.
///
/// It reads both spellings because cargo rewrites the manifest that goes
/// into a crates.io package with one table per dependency: a scan of the
/// first spelling alone finds `version` and `features` there, and no
/// dependency.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut in_deps: bool = false;
    for raw in manifest.lines() {
        let line: &str = raw.trim();
        if let Some(header) = line.strip_prefix('[') {
            let header: &str = header.trim_end_matches(']');
            // `[dependencies]` opens a table of names. `[dependencies.wgpu]`
            // is one dependency: the lines under it are its settings.
            in_deps = header.ends_with("dependencies");
            if let Some((table, name)) = header.rsplit_once('.') {
                if table.ends_with("dependencies") {
                    names.push(name.trim_matches('"').to_string());
                }
            }
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            names.push(key.trim().trim_matches('"').to_string());
        }
    }
    names
}

/// The names in `deps` that start with an entry of `OS_OR_GPU`.
fn forbidden(deps: &[String]) -> Vec<&String> {
    deps.iter().filter(|dep| OS_OR_GPU.iter().any(|f| dep.starts_with(f))).collect()
}

#[test]
fn gogo_text_names_no_os_or_gpu_crate() {
    let manifest: String = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).expect("Cargo.toml");
    let deps: Vec<String> = dependency_names(&manifest);
    assert!(deps.contains(&"cosmic-text".to_string()), "the scan reads this crate's dependencies: {deps:?}");
    let bad: Vec<&String> = forbidden(&deps);
    assert!(bad.is_empty(), "gogo-text must stay GPU- and window-free, but its Cargo.toml lists {bad:?}");
}

/// The scan is run on a manifest that breaks the rule.
///
/// Needed so that a green result above means "nothing forbidden is
/// there", never "the scan reads nothing".
#[test]
fn the_scan_sees_what_it_forbids() {
    let manifest: &str = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\nwgpu = { version = \"30\" }\n\n[dev-dependencies]\n# a comment\nandroid-activity = \"0.6\"\n\n[features]\ndefault = []\n";
    let deps: Vec<String> = dependency_names(manifest);
    assert_eq!(deps, ["serde", "wgpu", "android-activity"]);
    assert_eq!(forbidden(&deps), [&"wgpu".to_string(), &"android-activity".to_string()]);
    // The same names in the spelling cargo writes into a package.
    let packaged: &str = "[package]\nname = \"x\"\n\n[[test]]\nname = \"isolation\"\n\n[dependencies.serde]\nversion = \"1\"\nfeatures = [\n    \"derive\",\n]\n\n[dependencies.wgpu]\nversion = \"30\"\ndefault-features = false\n\n[dev-dependencies.android-activity]\nversion = \"0.6\"\n\n[target.'cfg(windows)'.dependencies]\nwinit = \"0.30\"\n";
    assert_eq!(dependency_names(packaged), ["serde", "wgpu", "android-activity", "winit"]);
}
