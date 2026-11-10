//! gogo-wgpu's two source rules as red-checks: it names no window or
//! phone crate, and its shader declares no `const` array.
//!
//! Needed because neither breakage shows on the PC a change is first
//! tried on. A window crate in the dependencies would force that window
//! library on every host, and a host with its own (a game engine, a
//! phone shell) could no longer use the renderer. A `const` array in the
//! shader compiles everywhere and draws wrong on one old phone GPU.

use std::fs;
use std::path::{Path, PathBuf};

/// Dependency names this crate must never list, in any table (prefix
/// match: `android` covers `android-activity` and `android_logger`). It
/// draws into a pass the host began; the window and the surface are the
/// host's.
const WINDOW_OR_OS: &[&str] = &["winit", "android", "jni", "ndk", "raw-window-handle", "egui", "sdl", "glfw"];

/// Every dependency name in a manifest's text: the keys of each
/// `[...dependencies...]` table, in order.
///
/// Needed as a plain line scan, with no toml crate, so that this test adds
/// no dependency of its own to the crate it guards.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut in_deps: bool = false;
    for raw in manifest.lines() {
        let line: &str = raw.trim();
        if line.starts_with('[') {
            in_deps = line.contains("dependencies");
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

/// The names in `deps` that start with an entry of `WINDOW_OR_OS`.
fn forbidden(deps: &[String]) -> Vec<&String> {
    deps.iter().filter(|dep| WINDOW_OR_OS.iter().any(|f| dep.starts_with(f))).collect()
}

/// The code lines of a shader source (comments stripped) that declare a
/// `const` array, as `line: code`.
fn const_arrays(source: &str) -> Vec<String> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let code: &str = line.split("//").next().unwrap_or("");
            code.trim_start().starts_with("const ") && code.contains("array<")
        })
        .map(|(i, line)| format!("{}: {}", i + 1, line.trim()))
        .collect()
}

#[test]
fn gogo_wgpu_owns_no_window() {
    let manifest: String = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).expect("Cargo.toml");
    let deps: Vec<String> = dependency_names(&manifest);
    assert!(deps.contains(&"wgpu".to_string()), "the scan reads this crate's dependencies: {deps:?}");
    let bad: Vec<&String> = forbidden(&deps);
    assert!(bad.is_empty(), "gogo-wgpu draws; the window is the host's. Its Cargo.toml lists {bad:?}");
}

/// No shader of this crate indexes a `const` array.
///
/// Needed because the Adreno 330's GLSL ES 3.0 compiler (a 2014 phone,
/// reached through wgpu's GL backend) returned zeros for half of a
/// `const` array looked up by vertex index: every quad drew as one
/// triangle. A function-local `var` array survives that driver, which is
/// what `ui.wgsl` uses.
#[test]
fn no_shader_declares_a_const_array() {
    let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut shaders: Vec<PathBuf> = fs::read_dir(&dir).expect("src").filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "wgsl")).collect();
    shaders.sort();
    assert!(!shaders.is_empty(), "no shader under {}: the src folder moved?", dir.display());
    for file in &shaders {
        let source: String = fs::read_to_string(file).expect("shader source");
        let bad: Vec<String> = const_arrays(&source);
        assert!(bad.is_empty(), "{} declares a const array; spell it as a function-local `var`:\n{}", file.display(), bad.join("\n"));
    }
}

/// The scans are run on inputs that break the rules.
///
/// Needed so that a green result above means "nothing forbidden is
/// there", never "the scan reads nothing".
#[test]
fn the_scans_see_what_they_forbid() {
    let manifest: &str = "[package]\nname = \"x\"\n\n[dependencies]\nwgpu = \"30\"\nwinit = \"0.30\"\n\n[dev-dependencies]\n# a comment\nandroid-activity = \"0.6\"\npollster = \"1\"\n";
    let deps: Vec<String> = dependency_names(manifest);
    assert_eq!(deps, ["wgpu", "winit", "android-activity", "pollster"]);
    assert_eq!(forbidden(&deps), [&"winit".to_string(), &"android-activity".to_string()]);

    let shader: &str = "// const CORNERS: array<vec2f, 6> in a comment is fine\nconst CORNERS: array<vec2f, 6> = array(vec2f(0.0, 0.0));\nfn f() { var corners = array<vec2f, 2>(vec2f(0.0), vec2f(1.0)); }\n";
    assert_eq!(const_arrays(shader), ["2: const CORNERS: array<vec2f, 6> = array(vec2f(0.0, 0.0));"]);
}
