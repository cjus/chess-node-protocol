//! `cnp-example-node`: the smallest correct CNP node.
//!
//! It serves no domain, so it answers only the lifecycle: `initialize`, `ping`, `shutdown` and
//! `exit` (spec, 5), with the errors the spec asks for around them. A real node starts from
//! this loop and adds its domain's methods where the comment says.
//!
//! One thread is enough here because every answer is immediate. A node with long requests (a
//! search) reads on while they run, so that `ping`, `engine/stop` and `$/cancelRequest` are
//! answered at once (spec, 4 and 5.2).

use std::io::{self, Write};
use std::process::ExitCode;

use cnp::lifecycle::{
    Capabilities, Exit, IncompatibleVersionData, Initialize, InitializeResult, Limits, NodeInfo,
    PROTOCOL_VERSION, Ping, Shutdown, negotiate,
};
use cnp::stdio::{read_message, write_message};
use cnp::{Message, Request, RequestMethod, Response, ResponseError, code};

/// Where the node is in its lifecycle.
#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Before `initialize`.
    Starting,
    /// Between `initialize` and `shutdown`.
    Running,
    /// After `shutdown`.
    ShutDown,
}

fn main() -> ExitCode {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut output = io::stdout().lock();
    let limits = Limits::default();
    let max_bytes = usize::try_from(limits.message_bytes()).unwrap_or(usize::MAX);
    let mut state = State::Starting;
    loop {
        let message = match read_message(&mut input, max_bytes) {
            // The client has gone: the same as `exit` (spec, 3.1).
            Ok(None) => return exit_code(state),
            Ok(Some(message)) => message,
            Err(e) => match e.response() {
                Some(response) => {
                    send(&mut output, response);
                    continue;
                }
                None => {
                    eprintln!("cnp-example-node: {e}");
                    return exit_code(state);
                }
            },
        };
        match message {
            Message::Request(request) => {
                let response = answer(&request, &mut state, &limits);
                send(&mut output, response);
            }
            Message::Notification(notification) if notification.is::<Exit>() => {
                return exit_code(state);
            }
            // `$/cancelRequest` included: every request here is answered at once, so there is
            // never one left to cancel. Unknown notifications are ignored (spec, 4).
            Message::Notification(_) => {}
            // This node sends no requests, so it expects no responses.
            Message::Response(_) => {}
        }
    }
}

/// The answer to one request.
fn answer(request: &Request, state: &mut State, limits: &Limits) -> Response {
    let id = request.id.clone();
    let error =
        |code, message: &str| Response::error(Some(id.clone()), ResponseError::new(code, message));
    match (*state, request.method.as_str()) {
        (State::Starting, Initialize::NAME) => {
            let params = match request.params::<Initialize>() {
                Ok(params) => params,
                Err(e) => return Response::error(Some(id), e),
            };
            let Some(protocol) = negotiate(params.protocol, PROTOCOL_VERSION) else {
                let data = IncompatibleVersionData {
                    supported: vec![PROTOCOL_VERSION],
                };
                return Response::error(
                    Some(id),
                    ResponseError::new(
                        code::INCOMPATIBLE_VERSION,
                        format!("this node speaks CNP {}.x", PROTOCOL_VERSION.major),
                    )
                    .with_data(&data),
                );
            };
            *state = State::Running;
            let result = InitializeResult {
                protocol,
                node: NodeInfo {
                    name: "cnp-example-node".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    author: None,
                    license: "MIT OR Apache-2.0".to_string(),
                    source: Some("https://github.com/cjus/chess-node-protocol".to_string()),
                },
                // A real node lists its domains here.
                capabilities: Capabilities::default(),
                limits: limits.clone(),
            };
            Response::ok::<Initialize>(id, &result)
        }
        (State::Starting, _) => error(code::NOT_INITIALIZED, "initialize first"),
        (_, Initialize::NAME) => error(code::INVALID_REQUEST, "already initialized"),
        (State::ShutDown, _) => error(code::SHUTTING_DOWN, "shutting down"),
        (State::Running, Ping::NAME) => Response::ok::<Ping>(id, &()),
        (State::Running, Shutdown::NAME) => {
            *state = State::ShutDown;
            Response::ok::<Shutdown>(id, &())
        }
        // A real node answers its domain's methods here.
        (State::Running, method) => error(code::METHOD_NOT_FOUND, &format!("no method {method}")),
    }
}

fn send(output: &mut impl Write, response: Response) {
    if let Err(e) = write_message(output, &response.into()) {
        eprintln!("cnp-example-node: can't write: {e}");
    }
}

/// 0 after `shutdown`, 1 otherwise (spec, 5.3).
fn exit_code(state: State) -> ExitCode {
    if state == State::ShutDown {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
