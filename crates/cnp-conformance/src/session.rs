//! Sessions: what to send a node, and what must come back, as JSON lines.
//!
//! A session is a `.jsonl` file. Its first line names it and says what it needs:
//!
//! ```json
//! {"session": "stop", "description": "engine/stop ends a search with its best move", "requires": ["engine"], "timeoutMs": 10000}
//! ```
//!
//! `requires` lists the capabilities a node must offer for the session to apply (see
//! [`crate::runner::capabilities`]): a domain (`engine`), one of its capabilities
//! (`engine.rootMoves`), or a limit (`limits.maxDepth`). `timeoutMs` is how long each step may
//! wait, 10 s if missing.
//!
//! Each line after it is one step, an object with one of these keys, and optionally a
//! `comment`:
//!
//! | Step | What it does |
//! | --- | --- |
//! | `{"send": message}` | Writes the message to the node |
//! | `{"sendRaw": "text"}` | Writes the text as a line, as it is (for broken messages) |
//! | `{"expect": pattern}` | Waits for a message matching the pattern ([`crate::pattern`]) |
//! | `{"expectAll": [pattern, …]}` | Waits for messages matching all of them, in any order |
//! | `{"expectExit": code}` | Waits for the node to end with this exit code (`null`: any) |
//! | `{"close": true}` | Closes the node's input |
//! | `{"comment": "text"}` | Nothing: a note for people |
//!
//! While waiting, notifications that don't match are skipped (a search's progress, a model's
//! state), but any other message that doesn't match fails the session.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};

/// How long a step waits unless the session says otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// A session, read from its file.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    /// Its name.
    pub name: String,
    /// What it checks, for people.
    pub description: Option<String>,
    /// The capabilities it needs.
    pub requires: Vec<String>,
    /// How long each step may wait.
    pub timeout: Duration,
    /// Its steps.
    pub steps: Vec<Step>,
    /// The file it came from.
    pub path: PathBuf,
}

/// One step of a session.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Write this message.
    Send(Value),
    /// Write this text as a line.
    SendRaw(String),
    /// Wait for a message matching this pattern.
    Expect(Value),
    /// Wait for messages matching all of these, in any order.
    ExpectAll(Vec<Value>),
    /// Wait for the node to end, with this exit code (`None`: any).
    ExpectExit(Option<i32>),
    /// Close the node's input.
    Close,
}

/// Reads a session from its text.
pub fn parse(text: &str, path: &Path) -> Result<Session, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (_, header) = lines.next().ok_or("an empty session")?;
    let header = object(header, 1)?;
    let name = header
        .get("session")
        .and_then(Value::as_str)
        .ok_or("the first line names the session: {\"session\": \"name\", …}")?
        .to_string();
    let description = header
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let requires = match header.get("requires") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or("requires lists strings")
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("requires is an array of strings".to_string()),
    };
    let timeout = match header.get("timeoutMs") {
        None => DEFAULT_TIMEOUT,
        Some(value) => Duration::from_millis(value.as_u64().ok_or("timeoutMs is a whole number")?),
    };
    let mut steps = Vec::new();
    for (index, line) in lines {
        let number = index + 1;
        let mut step = object(line, number)?;
        step.remove("comment");
        if step.len() > 1 {
            return Err(format!("line {number}: one step a line"));
        }
        let Some((key, value)) = step.into_iter().next() else {
            // Only a comment.
            continue;
        };
        steps.push(match (key.as_str(), value) {
            ("send", message) => Step::Send(message),
            ("sendRaw", Value::String(text)) => Step::SendRaw(text),
            ("expect", pattern) => Step::Expect(pattern),
            ("expectAll", Value::Array(patterns)) => Step::ExpectAll(patterns),
            ("expectExit", Value::Null) => Step::ExpectExit(None),
            ("expectExit", Value::Number(code)) => Step::ExpectExit(Some(
                code.as_i64()
                    .and_then(|c| i32::try_from(c).ok())
                    .ok_or(format!("line {number}: an exit code is a whole number"))?,
            )),
            ("close", Value::Bool(true)) => Step::Close,
            (key, _) => return Err(format!("line {number}: no step {key:?} of that form")),
        });
    }
    Ok(Session {
        name,
        description,
        requires,
        timeout,
        steps,
        path: path.to_path_buf(),
    })
}

fn object(line: &str, number: usize) -> Result<Map<String, Value>, String> {
    match serde_json::from_str(line) {
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => Err(format!("line {number}: a step is a JSON object")),
        Err(e) => Err(format!("line {number}: not JSON: {e}")),
    }
}

/// Reads the sessions in these files and directories (their `.jsonl` files, at any depth),
/// sorted by path.
pub fn load(paths: &[PathBuf]) -> Result<Vec<Session>, String> {
    let mut files = Vec::new();
    for path in paths {
        collect(path, &mut files)?;
    }
    files.sort();
    files
        .iter()
        .map(|file| {
            let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
            parse(&text, file).map_err(|e| format!("{}: {e}", file.display()))
        })
        .collect()
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_dir() {
        let entries = fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", path.display()))?;
            collect(&entry.path(), files)?;
        }
        Ok(())
    } else if path.is_file() {
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path.to_path_buf());
        }
        Ok(())
    } else {
        Err(format!("{}: no such file or directory", path.display()))
    }
}
