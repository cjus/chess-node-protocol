//! `cnp-conformance`: runs the conformance sessions against a node program.
//!
//! ```text
//! cnp-conformance [--timeout-ms N] <session file or directory>... -- <node program> [args...]
//! ```
//!
//! Prints a line for each session (PASS, SKIP or FAIL, with the reason) and a summary, and
//! exits with status 1 if any session failed.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use cnp_conformance::runner::{Outcome, capabilities, missing, run_session};
use cnp_conformance::session::{self, DEFAULT_TIMEOUT};

const USAGE: &str = "usage: cnp-conformance [--timeout-ms N] <session file or directory>... -- <node program> [args...]";

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("cnp-conformance: {e}");
            ExitCode::from(2)
        }
    }
}

/// Runs everything: `Ok(true)` if no session failed.
fn run() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split = args.iter().position(|a| a == "--").ok_or(USAGE)?;
    let (ours, command) = (&args[..split], &args[split + 1..]);
    if command.is_empty() {
        return Err(USAGE.to_string());
    }
    let mut timeout = DEFAULT_TIMEOUT;
    let mut paths = Vec::new();
    let mut ours = ours.iter();
    while let Some(arg) = ours.next() {
        match arg.as_str() {
            "--timeout-ms" => {
                let ms = ours.next().and_then(|v| v.parse().ok()).ok_or(USAGE)?;
                timeout = Duration::from_millis(ms);
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        return Err(USAGE.to_string());
    }
    let sessions = session::load(&paths)?;
    let offered = capabilities(command, timeout)?;
    let offers = if offered.is_empty() {
        "no domains, capabilities or limits".to_string()
    } else {
        offered.iter().cloned().collect::<Vec<_>>().join(", ")
    };
    println!("{} offers {offers}", command.join(" "));
    let (mut passed, mut skipped, mut failed) = (0, 0, 0);
    for session in &sessions {
        let outcome = match missing(session, &offered) {
            needs if !needs.is_empty() => Outcome::Skipped(needs),
            _ => run_session(session, command),
        };
        let name = session.path.display();
        match outcome {
            Outcome::Passed => {
                passed += 1;
                println!("PASS {name}");
            }
            Outcome::Skipped(needs) => {
                skipped += 1;
                println!("SKIP {name} (needs {})", needs.join(", "));
            }
            Outcome::Failed(why) => {
                failed += 1;
                println!("FAIL {name}");
                for line in why.lines() {
                    println!("     {line}");
                }
            }
        }
    }
    println!("{passed} passed, {skipped} skipped, {failed} failed");
    Ok(failed == 0)
}
