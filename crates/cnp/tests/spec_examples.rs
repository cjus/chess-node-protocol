//! Every example in the specification is a valid message, of the types this crate gives its
//! method, so the spec and the crate can't drift apart.
//!
//! The examples are the `cnp` blocks of `spec/cnp.md`: one message a line, after `→ ` (from
//! the client) or `← ` (from the node). A response is checked against the method of the
//! request with its id in the same block.

use std::collections::HashMap;

use cnp::{Id, Message, Notification, Request, RequestMethod, Response, engine, lifecycle, voice};

const SPEC: &str = include_str!("../../../spec/cnp.md");

/// The `cnp` blocks, each a list of its message lines.
fn blocks() -> Vec<Vec<&'static str>> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in SPEC.lines() {
        match (&mut current, line.trim_end()) {
            (None, "```cnp") => current = Some(Vec::new()),
            (Some(_), "```") => blocks.push(current.take().unwrap()),
            (Some(block), text) => block.push(text),
            (None, _) => {}
        }
    }
    assert!(current.is_none(), "a cnp block isn't closed");
    blocks
}

fn request_params(request: &Request) -> Result<(), String> {
    fn check<M: RequestMethod>(request: &Request) -> Result<(), String> {
        request.params::<M>().map(drop).map_err(|e| e.message)
    }
    match request.method.as_str() {
        "initialize" => check::<lifecycle::Initialize>(request),
        "ping" => check::<lifecycle::Ping>(request),
        "shutdown" => check::<lifecycle::Shutdown>(request),
        "engine/search" => check::<engine::Search>(request),
        "engine/stop" => check::<engine::Stop>(request),
        "engine/techniques" => check::<engine::Techniques>(request),
        "engine/levels" => check::<engine::Levels>(request),
        "voice/voices" => check::<voice::Voices>(request),
        "voice/speak" => check::<voice::Speak>(request),
        "voice/transcribe" => check::<voice::Transcribe>(request),
        "voice/models" => check::<voice::Models>(request),
        other => Err(format!("no method {other} in the crate")),
    }
}

fn response_result(method: &str, response: &Response) -> Result<(), String> {
    fn check<M: RequestMethod>(response: &Response) -> Result<(), String> {
        match response.outcome {
            // An error answers any method.
            Err(_) => Ok(()),
            Ok(_) => response.result::<M>().map(drop).map_err(|e| e.message),
        }
    }
    match method {
        "initialize" => check::<lifecycle::Initialize>(response),
        "ping" => check::<lifecycle::Ping>(response),
        "shutdown" => check::<lifecycle::Shutdown>(response),
        "engine/search" => check::<engine::Search>(response),
        "engine/stop" => check::<engine::Stop>(response),
        "engine/techniques" => check::<engine::Techniques>(response),
        "engine/levels" => check::<engine::Levels>(response),
        "voice/voices" => check::<voice::Voices>(response),
        "voice/speak" => check::<voice::Speak>(response),
        "voice/transcribe" => check::<voice::Transcribe>(response),
        "voice/models" => check::<voice::Models>(response),
        other => Err(format!("no method {other} in the crate")),
    }
}

fn notification_params(notification: &Notification) -> Result<(), String> {
    let result = match notification.method.as_str() {
        "exit" => notification.params::<lifecycle::Exit>().map(drop),
        "$/cancelRequest" => notification.params::<lifecycle::CancelRequest>().map(drop),
        "engine/progress" => notification.params::<engine::EngineProgress>().map(drop),
        "voice/modelState" => notification.params::<voice::ModelStateChanged>().map(drop),
        other => return Err(format!("no notification {other} in the crate")),
    };
    result.map_err(|e| e.message)
}

#[test]
fn every_example_in_the_spec_is_a_valid_typed_message() {
    let blocks = blocks();
    assert!(blocks.len() >= 10, "only {} cnp blocks found", blocks.len());
    let mut checked = 0;
    for (n, block) in blocks.iter().enumerate() {
        let mut methods: HashMap<Id, String> = HashMap::new();
        for line in block {
            let text = line
                .strip_prefix("→ ")
                .or_else(|| line.strip_prefix("← "))
                .unwrap_or_else(|| panic!("block {n}: a line without an arrow: {line}"));
            let message: Message = text
                .parse()
                .unwrap_or_else(|e| panic!("block {n}: {e}: {text}"));
            let outcome = match &message {
                Message::Request(request) => {
                    methods.insert(request.id.clone(), request.method.clone());
                    request_params(request)
                }
                Message::Notification(notification) => notification_params(notification),
                Message::Response(response) => {
                    let id = response.id.as_ref().expect("examples' responses have ids");
                    let method = methods
                        .get(id)
                        .unwrap_or_else(|| panic!("block {n}: no request {id} before: {text}"));
                    response_result(method, response)
                }
            };
            if let Err(e) = outcome {
                panic!("block {n}: {e}: {text}");
            }
            checked += 1;
        }
    }
    assert!(checked >= 25, "only {checked} messages checked");
}
