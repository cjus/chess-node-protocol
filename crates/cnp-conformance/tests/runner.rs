//! The runner against the example node: the lifecycle sessions pass, and a session that
//! expects the wrong thing fails, saying why.

use std::path::{Path, PathBuf};
use std::time::Duration;

use cnp::Message;
use cnp_conformance::runner::{Outcome, capabilities, missing, run_session};
use cnp_conformance::session::{self, Step};

fn example_node() -> Vec<String> {
    vec![env!("CARGO_BIN_EXE_cnp-example-node").to_string()]
}

fn conformance_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

fn run(text: &str) -> Outcome {
    let session = session::parse(text, Path::new("inline.jsonl")).expect("a valid session");
    run_session(&session, &example_node())
}

const INIT: &str = r#"{"send": {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocol": "1.0", "client": {"name": "test", "version": "1"}}}}
{"expect": {"id": 1, "result": {"protocol": "1.0"}}}"#;

#[test]
fn every_lifecycle_session_passes_against_the_example_node() {
    let sessions = session::load(&[conformance_dir().join("lifecycle")]).unwrap();
    assert!(
        sessions.len() >= 10,
        "only {} lifecycle sessions",
        sessions.len()
    );
    let offered = capabilities(&example_node(), Duration::from_secs(10)).unwrap();
    for session in &sessions {
        assert_eq!(
            missing(session, &offered),
            Vec::<String>::new(),
            "{}",
            session.name
        );
        assert_eq!(
            run_session(session, &example_node()),
            Outcome::Passed,
            "{}",
            session.name
        );
    }
}

#[test]
fn the_example_node_offers_no_domain() {
    let offered = capabilities(&example_node(), Duration::from_secs(10)).unwrap();
    assert!(
        offered.iter().all(|name| name.starts_with("limits.")),
        "{offered:?}"
    );
}

#[test]
fn every_session_file_reads_and_sends_valid_messages() {
    let sessions = session::load(&[conformance_dir()]).unwrap();
    assert!(sessions.len() >= 40, "only {} sessions", sessions.len());
    for session in &sessions {
        assert!(
            session.description.is_some(),
            "{} has no description",
            session.name
        );
        for step in &session.steps {
            if let Step::Send(message) = step {
                Message::from_value(message.clone())
                    .unwrap_or_else(|e| panic!("{}: {e}: {message}", session.name));
            }
        }
    }
}

#[test]
fn a_wrong_result_fails_and_says_what_came() {
    let outcome = run(&format!(
        "{{\"session\": \"wrong\"}}\n{INIT}\n\
         {{\"send\": {{\"jsonrpc\": \"2.0\", \"id\": 2, \"method\": \"ping\"}}}}\n\
         {{\"expect\": {{\"id\": 2, \"result\": 1}}}}"
    ));
    let Outcome::Failed(why) = outcome else {
        panic!("passed: {outcome:?}");
    };
    assert!(why.contains("step 4"), "{why}");
    assert!(why.contains("\"result\":null"), "{why}");
}

#[test]
fn a_wrong_exit_code_fails() {
    let outcome = run(&format!(
        "{{\"session\": \"exit\"}}\n{INIT}\n\
         {{\"send\": {{\"jsonrpc\": \"2.0\", \"method\": \"exit\"}}}}\n\
         {{\"expectExit\": 0}}"
    ));
    let Outcome::Failed(why) = outcome else {
        panic!("passed: {outcome:?}");
    };
    assert!(why.contains("exit code 1"), "{why}");
}

#[test]
fn waiting_for_a_message_that_never_comes_times_out() {
    let outcome = run(&format!(
        "{{\"session\": \"silence\", \"timeoutMs\": 200}}\n{INIT}\n\
         {{\"expect\": {{\"id\": 2, \"result\": null}}}}"
    ));
    let Outcome::Failed(why) = outcome else {
        panic!("passed: {outcome:?}");
    };
    assert!(why.contains("nothing after 200 ms"), "{why}");
}

#[test]
fn bound_values_are_checked_later_in_the_session() {
    let ping = |id: u32| {
        format!("{{\"send\": {{\"jsonrpc\": \"2.0\", \"id\": {id}, \"method\": \"ping\"}}}}")
    };
    let passes = run(&format!(
        "{{\"session\": \"bind\"}}\n{INIT}\n{}\n\
         {{\"expect\": {{\"id\": 2, \"result\": \"$bind:r\"}}}}\n{}\n\
         {{\"expect\": {{\"id\": 3, \"result\": \"$ref:r\"}}}}",
        ping(2),
        ping(3)
    ));
    assert_eq!(passes, Outcome::Passed);
}
