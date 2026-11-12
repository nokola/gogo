//! The `storvald` command, run as a program.
//!
//! Needed because a script sees the exit code and the two output streams of
//! the built program, and a call of a function inside it shows neither.

use std::process::{Command, Output};

/// Runs the built `storvald` with `args` and returns what it left behind.
fn storvald(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_storvald")).args(args).output().expect("the built command runs")
}

/// A bare call, and a call for help or for the version, says what this
/// version is and succeeds.
#[test]
fn a_call_that_asks_for_nothing_says_what_this_version_is() {
    let calls: [&[&str]; 5] = [&[], &["--version"], &["-V"], &["--help"], &["-h"]];
    for args in calls {
        let out: Output = storvald(args);
        assert!(out.status.success(), "{args:?}");
        let said: String = String::from_utf8(out.stdout).expect("text");
        assert!(said.starts_with(concat!("storvald ", env!("CARGO_PKG_VERSION"))), "{args:?}: {said}");
        assert!(said.contains("in development"), "{args:?}: {said}");
    }
}

/// A command this version does not have fails, and says which one was asked
/// for, so a script that calls it does not take a call that did nothing for
/// a pass.
#[test]
fn a_command_that_does_not_exist_yet_fails() {
    let out: Output = storvald(&["go"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "nothing on the stream a script reads results from");
    let said: String = String::from_utf8(out.stderr).expect("text");
    assert!(said.contains("`go` is not a command yet"), "{said}");
}
