//! Every crate this repository publishes carries the two license texts,
//! byte for byte the repository's own.
//!
//! Needed because crates.io packs a crate from its own folder: the
//! `LICENSE-MIT` and `LICENSE-APACHE` at the top of the repository are not
//! in a package, and both licenses ask that their text goes along with
//! every copy of the code. A new crate added without the two files, or a
//! copy edited on its own, passes every other test.

use std::fs;
use std::path::{Path, PathBuf};

/// The license files, by the names crates.io and GitHub look for.
const LICENSES: [&str; 2] = ["LICENSE-MIT", "LICENSE-APACHE"];

/// The crate folders under `repo/crates`: every folder there that holds a
/// `Cargo.toml`, sorted by path.
fn crate_dirs(repo: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(repo.join("crates")).expect("the crates folder").flatten().map(|entry| entry.path()).filter(|path| path.join("Cargo.toml").is_file()).collect();
    dirs.sort();
    dirs
}

/// What is wrong with the license files of the crates under `repo`, one
/// line per file: missing, or not the bytes of the repository's own.
///
/// Returns an empty list when every crate carries both texts.
fn license_problems(repo: &Path) -> Vec<String> {
    let mut problems: Vec<String> = Vec::new();
    for name in LICENSES {
        let original: Vec<u8> = fs::read(repo.join(name)).unwrap_or_else(|e| panic!("{name} at the top of the repository: {e}"));
        for dir in crate_dirs(repo) {
            let crate_name: String = dir.file_name().expect("a folder name").to_string_lossy().into_owned();
            match fs::read(dir.join(name)) {
                Ok(copy) if copy == original => {}
                Ok(_) => problems.push(format!("crates/{crate_name}/{name} differs from the repository's {name}")),
                Err(_) => problems.push(format!("crates/{crate_name}/{name} is missing")),
            }
        }
    }
    problems
}

#[test]
fn every_crate_carries_both_license_texts() {
    let repo: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let crates: usize = crate_dirs(&repo).len();
    assert!(crates >= 3, "only {crates} crate folders under {}: did the crates folder move?", repo.join("crates").display());
    for name in LICENSES {
        let bytes: usize = fs::read(repo.join(name)).expect("the repository's license file").len();
        assert!(bytes > 1000, "{name} holds {bytes} bytes: not a license text");
    }
    let problems: Vec<String> = license_problems(&repo);
    assert!(problems.is_empty(), "copy the file from the top of the repository into the crate's folder:\n{}", problems.join("\n"));
}

/// The check is run on a made-up repository with one good crate, one
/// crate without the files and one with an edited copy.
///
/// Needed so that a green result above means "every copy is there and
/// equal", never "the check found no crate to look at".
#[test]
fn the_check_sees_a_missing_and_an_edited_copy() {
    let repo: PathBuf = std::env::temp_dir().join(format!("gogo-license-check-{}", std::process::id()));
    let _ = fs::remove_dir_all(&repo);
    for name in ["good", "bare", "edited"] {
        fs::create_dir_all(repo.join("crates").join(name)).expect("a crate folder");
        fs::write(repo.join("crates").join(name).join("Cargo.toml"), "[package]\n").expect("a manifest");
    }
    // A folder with no manifest is no crate: it needs no license files.
    fs::create_dir_all(repo.join("crates").join("notes")).expect("a plain folder");
    for name in LICENSES {
        fs::write(repo.join(name), format!("the text of {name}\n")).expect("the repository's file");
        fs::write(repo.join("crates").join("good").join(name), format!("the text of {name}\n")).expect("a good copy");
        fs::write(repo.join("crates").join("edited").join(name), format!("the text of {name}, edited\n")).expect("an edited copy");
    }
    let problems: Vec<String> = license_problems(&repo);
    let _ = fs::remove_dir_all(&repo);
    assert_eq!(problems, ["crates/bare/LICENSE-MIT is missing", "crates/edited/LICENSE-MIT differs from the repository's LICENSE-MIT", "crates/bare/LICENSE-APACHE is missing", "crates/edited/LICENSE-APACHE differs from the repository's LICENSE-APACHE"]);
}
