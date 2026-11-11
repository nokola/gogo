//! gogo-ui's isolation rules as red-checks: it names no OS or GPU crate,
//! and its runtime reads no disk and no environment — unless a host turns
//! the `hot-reload` feature on, which reads the host's page folder in a
//! development build.
//!
//! Needed because gogo has to run wherever a host can draw rectangles and
//! text (a phone, a browser, a test with no GPU), and a new dependency or
//! a stray `std::fs` call builds fine on the PC it is first tried on, so
//! nothing else goes red when that promise breaks.
//!
//! The rules cover this crate only: a renderer or a text crate in this
//! repository names its GPU or font library on purpose.

use std::fs;
use std::path::{Path, PathBuf};

/// Dependency names this crate must never list (prefix match: `android`
/// covers `android-activity` and `android_logger`).
const OS_OR_GPU: &[&str] = &["wgpu", "winit", "android", "jni", "image", "egui", "pollster", "env_logger", "raw-window-handle", "ndk"];

/// Source files that read the disk on purpose: `lib.rs` holds the
/// compiler's front door (`compile_file`, `build_pages`), which a host's
/// build script calls at BUILD time. A running app never calls it.
const FS_ALLOWED: &[&str] = &["lib.rs"];

/// Source files whose disk reads sit behind a cargo feature, each with
/// the lines that open the feature's module: `reload.rs` reads a host's
/// page folder inside `mod live`, compiled only with `hot-reload` (off by
/// default; a development build turns it on). The scan covers the file up
/// to that module, which must be the file's last item so that nothing
/// after it goes unscanned.
const FS_BEHIND_FEATURE: &[(&str, &str)] = &[("reload.rs", "#[cfg(feature = \"hot-reload\")]\nmod live {\n")];

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

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir").flatten() {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// The `std::fs` / `std::env` mentions in a source text, as
/// `name:line: code`, outside comments and above the first `#[cfg(test)]`
/// (`env!` / `option_env!` are compile-time macros and allowed).
fn os_uses(name: &str, source: &str) -> Vec<String> {
    let mut hits: Vec<String> = Vec::new();
    for (i, line) in source.lines().enumerate() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let code: &str = line.split("//").next().unwrap_or("");
        if code.contains("std::fs") || code.contains("std::env::") {
            hits.push(format!("{name}:{}: {}", i + 1, line.trim()));
        }
    }
    hits
}

/// The part of `source` every build compiles: the text before `opens`,
/// the lines that open a module compiled only with a cargo feature.
///
/// Returns `Err` when the module is not there, or is not the file's last
/// item (code after it would be compiled in every build and not scanned).
fn part_in_every_build<'a>(name: &str, source: &'a str, opens: &str) -> Result<&'a str, String> {
    let Some(at) = source.find(opens) else { return Err(format!("{name} no longer holds the lines that open its feature's module: {opens:?}")) };
    let module: &str = &source[at + opens.len()..];
    // The module's own closing brace is the first `}` that starts a line.
    let Some(close) = module.find("\n}\n") else { return Err(format!("{name}: the feature's module does not close")) };
    if !module[close + 3..].trim().is_empty() {
        return Err(format!("{name}: code follows the feature's module and would go unscanned"));
    }
    Ok(&source[..at])
}

#[test]
fn gogo_ui_names_no_os_or_gpu_crate() {
    let manifest: String = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).expect("Cargo.toml");
    let deps: Vec<String> = dependency_names(&manifest);
    let bad: Vec<&String> = forbidden(&deps);
    assert!(bad.is_empty(), "gogo-ui must stay OS- and GPU-free, but its Cargo.toml lists {bad:?}");
}

#[test]
fn gogo_ui_touches_no_disk_outside_tests() {
    let src: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<PathBuf> = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() >= 10, "only {} source files under {}: the src folder moved?", files.len(), src.display());
    let mut bad: Vec<String> = Vec::new();
    for file in &files {
        let rel: String = file.strip_prefix(&src).expect("under src").to_string_lossy().replace('\\', "/");
        if FS_ALLOWED.contains(&rel.as_str()) {
            continue;
        }
        let source: String = fs::read_to_string(file).expect("source");
        let scanned: &str = match FS_BEHIND_FEATURE.iter().find(|(name, _)| *name == rel) {
            Some((_, opens)) => part_in_every_build(&rel, &source, opens).unwrap_or_else(|why| panic!("{why}")),
            None => &source,
        };
        bad.extend(os_uses(&rel, scanned));
    }
    assert!(bad.is_empty(), "gogo-ui reads no disk and no environment at runtime (outside a feature's module):\n{}", bad.join("\n"));
}

/// The two scans are run on inputs that break the rules.
///
/// Needed so that a green result above means "nothing forbidden is
/// there", never "the scan reads nothing": this crate has no dependency
/// today, and only `lib.rs` reads the disk.
#[test]
fn the_scans_see_what_they_forbid() {
    let manifest: &str = "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\nwgpu = { version = \"30\" }\n\n[dev-dependencies]\n# a comment\nandroid-activity = \"0.6\"\n\n[features]\ndefault = []\n";
    let deps: Vec<String> = dependency_names(manifest);
    assert_eq!(deps, ["serde", "wgpu", "android-activity"]);
    assert_eq!(forbidden(&deps), [&"wgpu".to_string(), &"android-activity".to_string()]);
    // The same names in the spelling cargo writes into a package.
    let packaged: &str = "[package]\nname = \"x\"\n\n[[test]]\nname = \"isolation\"\n\n[dependencies.serde]\nversion = \"1\"\nfeatures = [\n    \"derive\",\n]\n\n[dependencies.wgpu]\nversion = \"30\"\ndefault-features = false\n\n[dev-dependencies.android-activity]\nversion = \"0.6\"\n\n[target.'cfg(windows)'.dependencies]\nwinit = \"0.30\"\n";
    assert_eq!(dependency_names(packaged), ["serde", "wgpu", "android-activity", "winit"]);

    let source: &str = "use std::path::Path;\n// std::fs in a comment is fine\nlet v = env!(\"CARGO_PKG_VERSION\");\nlet t = std::fs::read_to_string(p);\nlet home = std::env::var(\"HOME\");\n#[cfg(test)]\nmod tests { fn f() { std::fs::read(\"x\").unwrap(); } }\n";
    let hits: Vec<String> = os_uses("x.rs", source);
    assert_eq!(hits, ["x.rs:4: let t = std::fs::read_to_string(p);", "x.rs:5: let home = std::env::var(\"HOME\");"]);

    // A file with a feature's module: a read before the module is seen,
    // a read inside it is not, and code after the module is refused.
    let opens: &str = "#[cfg(feature = \"x\")]\nmod live {\n";
    let gated: String = format!("fn a() {{ std::fs::read(\"a\"); }}\n{opens}    fn b() {{ std::fs::read(\"b\"); }}\n}}\n");
    assert_eq!(os_uses("g.rs", part_in_every_build("g.rs", &gated, opens).unwrap()), ["g.rs:1: fn a() { std::fs::read(\"a\"); }"]);
    assert!(part_in_every_build("g.rs", &format!("{gated}fn c() {{ std::fs::read(\"c\"); }}\n"), opens).unwrap_err().contains("follows"));
    assert!(part_in_every_build("g.rs", "fn a() {}\n", opens).unwrap_err().contains("no longer holds"));
    let reload: String = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("reload.rs")).expect("reload.rs");
    assert!(!os_uses("reload.rs", &reload).is_empty(), "reload.rs is listed because its feature's module reads the disk; if it no longer does, drop it from FS_BEHIND_FEATURE");

    let lib: String = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("lib.rs")).expect("lib.rs");
    assert!(!os_uses("lib.rs", &lib).is_empty(), "lib.rs is exempt because it reads the disk; if it no longer does, drop it from FS_ALLOWED");
}
