//! The `storvald` command.
//!
//! Storvald is in development (<https://github.com/nokola/storvald>). This
//! version holds the crate name and the command name, and runs nothing.

use std::process::ExitCode;

/// What the command says on every call, until it has commands of its own.
const NOTE: &str = concat!("storvald ", env!("CARGO_PKG_VERSION"), ": in development, there is nothing to run yet.\nhttps://github.com/nokola/storvald");

/// Whether a call asks the command to do work, as `storvald go` does.
///
/// Needed so a script or an AI agent that calls a command this version does
/// not have gets a failure, not a success that did nothing.
///
/// Returns false for a bare `storvald` and for a call with only `--help`,
/// `-h`, `--version` or `-V`.
fn asks_for_work(args: &[String]) -> bool {
    !args.iter().all(|arg| matches!(arg.as_str(), "--help" | "-h" | "--version" | "-V"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if asks_for_work(&args) {
        eprintln!("{NOTE}");
        eprintln!("storvald: `{}` is not a command yet", args.join(" "));
        return ExitCode::from(2);
    }
    println!("{NOTE}");
    ExitCode::SUCCESS
}
