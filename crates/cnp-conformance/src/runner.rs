//! Running sessions against a node program.
//!
//! Each session gets a fresh node: [`Node::start`] runs the program as a child process, and a
//! thread reads its output, so a step can wait with a deadline. A session fails at its first
//! step that goes wrong, and the end of the node's standard error is kept to show why.
//!
//! Before the sessions, [`capabilities`] starts the node once, asks `initialize`, and lists
//! what it offers, so sessions that need more are skipped rather than failed.

use std::collections::BTreeSet;
use std::io::{BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use cnp::lifecycle::{Initialize, InitializeParams, PROTOCOL_VERSION, Software};
use cnp::stdio::{ReadError, read_message};
use cnp::{Message, Request};
use serde_json::{Value, json};

use crate::pattern::{Bindings, fill, matches};
use crate::session::{Session, Step};

/// The longest message the runner reads from a node: well above any node's needs.
const MAX_MESSAGE_BYTES: usize = 256 * 1024 * 1024;

/// How many lines of a failed node's standard error to show.
const STDERR_LINES: usize = 20;

/// How often to look whether the node has ended.
const EXIT_POLL: Duration = Duration::from_millis(10);

/// How a session went.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Every step went as expected.
    Passed,
    /// It needs capabilities the node doesn't offer: these.
    Skipped(Vec<String>),
    /// A step went wrong: why.
    Failed(String),
}

/// What the reading thread passes on.
enum Received {
    Message(Value),
    Invalid(String),
    End,
}

/// A node program running as a child process.
pub struct Node {
    child: Child,
    stdin: Option<ChildStdin>,
    received: Receiver<Received>,
    stderr: Arc<Mutex<Vec<u8>>>,
    /// Notifications skipped while waiting, for a failure's report.
    skipped: Vec<Value>,
}

impl Node {
    /// Starts `command` (the program, then its arguments).
    pub fn start(command: &[String]) -> Result<Node, String> {
        let (program, args) = command.split_first().ok_or("no node command")?;
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("can't start {program}: {e}"))?;
        let stdout = child.stdout.take().expect("piped");
        let (send, received) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let item = match read_message(&mut reader, MAX_MESSAGE_BYTES) {
                    Ok(Some(message)) => Received::Message(message.to_value()),
                    Ok(None) | Err(ReadError::Io(_)) => Received::End,
                    Err(e) => Received::Invalid(e.to_string()),
                };
                let end = matches!(item, Received::End);
                if send.send(item).is_err() || end {
                    return;
                }
            }
        });
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let mut pipe = child.stderr.take().expect("piped");
        let sink = Arc::clone(&stderr);
        thread::spawn(move || {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut chunk) {
                if n == 0 {
                    return;
                }
                sink.lock()
                    .expect("not poisoned")
                    .extend_from_slice(&chunk[..n]);
            }
        });
        let stdin = child.stdin.take();
        Ok(Node {
            child,
            stdin,
            received,
            stderr,
            skipped: Vec::new(),
        })
    }

    /// Writes one line to the node.
    pub fn send_line(&mut self, line: &str) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("the node's input is closed")?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.write_all(b"\n"))
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("can't write to the node: {e}"))
    }

    /// Closes the node's input.
    pub fn close(&mut self) {
        self.stdin = None;
    }

    /// Waits for a message matching `pattern`, skipping notifications that don't.
    pub fn expect(
        &mut self,
        pattern: &Value,
        bindings: &mut Bindings,
        timeout: Duration,
    ) -> Result<(), String> {
        self.expect_all(std::slice::from_ref(pattern), bindings, timeout)
    }

    /// Waits for messages matching all of `patterns`, in any order, skipping notifications
    /// that match none.
    pub fn expect_all(
        &mut self,
        patterns: &[Value],
        bindings: &mut Bindings,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut waiting: Vec<&Value> = patterns.iter().collect();
        while !waiting.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = match self.received.recv_timeout(left) {
                Ok(Received::Message(message)) => message,
                Ok(Received::Invalid(e)) => return Err(format!("the node sent {e}")),
                Ok(Received::End) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(format!(
                        "the node's output ended while waiting for {}",
                        list(&waiting)
                    ));
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "nothing after {} ms while waiting for {}",
                        timeout.as_millis(),
                        list(&waiting)
                    ));
                }
            };
            let mut mismatches = Vec::new();
            let found =
                waiting
                    .iter()
                    .position(|pattern| match matches(pattern, &message, bindings) {
                        Ok(()) => true,
                        Err(mismatch) => {
                            mismatches.push(mismatch.to_string());
                            false
                        }
                    });
            match found {
                Some(i) => {
                    waiting.remove(i);
                }
                None if is_notification(&message) => self.skipped.push(message),
                None => {
                    return Err(format!(
                        "got {message}\nexpected {}\n({})",
                        list(&waiting),
                        mismatches.join("; ")
                    ));
                }
            }
        }
        Ok(())
    }

    /// Waits for the node to end, and gives its exit code (`None` if a signal ended it).
    pub fn wait_exit(&mut self, timeout: Duration) -> Result<Option<i32>, String> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status.code()),
                Ok(None) if Instant::now() >= deadline => {
                    return Err(format!(
                        "the node was still running after {} ms",
                        timeout.as_millis()
                    ));
                }
                Ok(None) => thread::sleep(EXIT_POLL),
                Err(e) => return Err(format!("can't wait for the node: {e}")),
            }
        }
    }

    /// The last lines the node wrote to its standard error.
    pub fn stderr_tail(&self) -> String {
        let bytes = self.stderr.lock().expect("not poisoned").clone();
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(STDERR_LINES)..].join("\n")
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        // A session may end with the node still running; it mustn't outlive the runner.
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// Plays one session against a fresh node.
pub fn run_session(session: &Session, command: &[String]) -> Outcome {
    let mut node = match Node::start(command) {
        Ok(node) => node,
        Err(e) => return Outcome::Failed(e),
    };
    let mut bindings = Bindings::new();
    for (n, step) in session.steps.iter().enumerate() {
        let result = match step {
            Step::Send(message) => {
                fill(message, &bindings).and_then(|message| node.send_line(&message.to_string()))
            }
            Step::SendRaw(text) => node.send_line(text),
            Step::Expect(pattern) => node.expect(pattern, &mut bindings, session.timeout),
            Step::ExpectAll(patterns) => node.expect_all(patterns, &mut bindings, session.timeout),
            Step::ExpectExit(code) => match node.wait_exit(session.timeout) {
                Ok(actual) if code.is_none() || actual == *code => Ok(()),
                Ok(actual) => Err(format!(
                    "the node ended with exit code {}, expected {}",
                    actual.map_or("none (a signal)".to_string(), |c| c.to_string()),
                    code.map_or("any".to_string(), |c| c.to_string())
                )),
                Err(e) => Err(e),
            },
            Step::Close => {
                node.close();
                Ok(())
            }
        };
        if let Err(e) = result {
            let mut report = format!("step {}: {e}", n + 1);
            if !node.skipped.is_empty() {
                report.push_str(&format!("\nskipped notifications: {}", node.skipped.len()));
            }
            let stderr = node.stderr_tail();
            if !stderr.is_empty() {
                report.push_str(&format!("\nthe node's stderr ends:\n{stderr}"));
            }
            return Outcome::Failed(report);
        }
    }
    Outcome::Passed
}

/// What a node offers, as names sessions can require: each domain it serves (`engine`), each
/// capability that is `true` (`engine.rootMoves`), and each limit it sets (`limits.maxDepth`).
pub fn capabilities(command: &[String], timeout: Duration) -> Result<BTreeSet<String>, String> {
    let mut node = Node::start(command)?;
    let params = InitializeParams {
        protocol: PROTOCOL_VERSION,
        client: Software {
            name: "cnp-conformance".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    };
    let request = Message::from(Request::new::<Initialize>(1, &params));
    node.send_line(&request.to_value().to_string())?;
    let mut bindings = Bindings::new();
    let pattern = json!({"id": 1, "result": "$bind:result"});
    node.expect(&pattern, &mut bindings, timeout)
        .map_err(|e| format!("initialize: {e}"))?;
    let _ = node.send_line(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#);
    let _ = node.send_line(r#"{"jsonrpc":"2.0","method":"exit"}"#);
    let result = &bindings["result"];
    let mut names = BTreeSet::new();
    if let Some(Value::Object(domains)) = result.get("capabilities") {
        for (domain, capabilities) in domains {
            let Value::Object(capabilities) = capabilities else {
                continue;
            };
            names.insert(domain.clone());
            for (name, value) in capabilities {
                if value == &Value::Bool(true) {
                    names.insert(format!("{domain}.{name}"));
                }
            }
        }
    }
    if let Some(Value::Object(limits)) = result.get("limits") {
        for (name, value) in limits {
            if !value.is_null() {
                names.insert(format!("limits.{name}"));
            }
        }
    }
    Ok(names)
}

/// The capabilities a session needs that the node doesn't offer.
pub fn missing(session: &Session, offered: &BTreeSet<String>) -> Vec<String> {
    session
        .requires
        .iter()
        .filter(|name| !offered.contains(*name))
        .cloned()
        .collect()
}

fn is_notification(message: &Value) -> bool {
    message.get("method").is_some() && message.get("id").is_none()
}

fn list(patterns: &[&Value]) -> String {
    patterns
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(" and ")
}
