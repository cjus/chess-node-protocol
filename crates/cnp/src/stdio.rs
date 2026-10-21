//! The stdin/stdout connection (spec, "Connections"): one message per line.
//!
//! A client starts a node as a child process and talks to it through its standard input and
//! output. Each message is one line of UTF-8 JSON ending in `\n`; JSON escapes line breaks
//! inside strings, so a message never spans lines. The node writes its log to standard error,
//! never to standard output. When the node's input ends, the client has gone, and the node
//! ends too.
//!
//! [`read_message`] reads the next message, skipping blank lines (handy when typing a session
//! by hand), and [`write_message`] writes one and flushes it.
//!
//! ## Pitfalls
//!
//! - **A line can be any length**, so reading one without a limit lets a broken or hostile
//!   peer use up all memory. [`read_message`] stops collecting at `max_bytes`, skips the rest
//!   of the line, and reports [`ReadError::TooLong`].
//! - **Flush after every message.** A message sitting in a buffer is a message the other side
//!   waits for forever.
//! - **Anything else on standard output breaks the connection**: a stray `println!` in a node
//!   is a message the client can't read.

use std::fmt;
use std::io::{self, BufRead, Write};

use serde_json::Value;

use crate::message::{InvalidMessage, Message, Response, ResponseError, code};

/// Why [`read_message`] returned no message.
#[derive(Debug)]
pub enum ReadError {
    /// Reading failed.
    Io(io::Error),
    /// The line was longer than allowed; the rest of it was skipped.
    TooLong {
        /// The limit it went over, in bytes.
        max_bytes: usize,
    },
    /// The line isn't JSON (or isn't UTF-8).
    Parse(serde_json::Error),
    /// The line is JSON, but not a valid message.
    Invalid(InvalidMessage),
}

impl ReadError {
    /// The response a node sends back, if there is one to send: a parse error with a `null`
    /// id, or an invalid request with the id if it could be read. Nothing for a failed read.
    pub fn response(&self) -> Option<Response> {
        match self {
            ReadError::Io(_) => None,
            ReadError::TooLong { max_bytes } => Some(Response::error(
                None,
                ResponseError::new(
                    code::INVALID_REQUEST,
                    format!("a message is at most {max_bytes} bytes"),
                ),
            )),
            ReadError::Parse(e) => Some(Response::error(
                None,
                ResponseError::new(code::PARSE_ERROR, format!("not JSON: {e}")),
            )),
            ReadError::Invalid(invalid) => Some(Response::error(
                invalid.id.clone(),
                ResponseError::new(code::INVALID_REQUEST, invalid.to_string()),
            )),
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "reading failed: {e}"),
            ReadError::TooLong { max_bytes } => write!(f, "a line over {max_bytes} bytes"),
            ReadError::Parse(e) => write!(f, "not JSON: {e}"),
            ReadError::Invalid(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<io::Error> for ReadError {
    fn from(e: io::Error) -> ReadError {
        ReadError::Io(e)
    }
}

/// Reads the next message: `Ok(None)` when the input has ended.
///
/// After an error other than [`ReadError::Io`], the reader is at the start of the next line,
/// so reading can go on.
pub fn read_message(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> Result<Option<Message>, ReadError> {
    let mut line = Vec::new();
    loop {
        match read_line(reader, max_bytes, &mut line)? {
            Line::End => return Ok(None),
            Line::TooLong => return Err(ReadError::TooLong { max_bytes }),
            Line::Complete if line.iter().all(u8::is_ascii_whitespace) => continue,
            Line::Complete => {
                let value: Value = serde_json::from_slice(&line).map_err(ReadError::Parse)?;
                return Message::from_value(value)
                    .map(Some)
                    .map_err(ReadError::Invalid);
            }
        }
    }
}

/// Writes one message as a line, and flushes it.
pub fn write_message(writer: &mut impl Write, message: &Message) -> io::Result<()> {
    // Compact JSON has no line breaks: strings escape theirs.
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// What [`read_line`] found.
enum Line {
    /// A line, in the buffer, without its line ending.
    Complete,
    /// A line over the limit, now skipped.
    TooLong,
    /// The end of the input, with nothing before it.
    End,
}

/// Reads one line into `line`, without its `\n` or `\r\n`, holding at most `max_bytes`.
fn read_line(reader: &mut impl BufRead, max_bytes: usize, line: &mut Vec<u8>) -> io::Result<Line> {
    line.clear();
    let mut too_long = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            // The input ended: a last line without its `\n` still counts.
            return Ok(if too_long {
                Line::TooLong
            } else if line.is_empty() {
                Line::End
            } else {
                Line::Complete
            });
        }
        let (chunk, ended) = match available.iter().position(|&b| b == b'\n') {
            Some(at) => (&available[..at], true),
            None => (available, false),
        };
        if !too_long && line.len() + chunk.len() <= max_bytes {
            line.extend_from_slice(chunk);
        } else {
            too_long = true;
            line.clear();
        }
        let used = chunk.len() + usize::from(ended);
        reader.consume(used);
        if ended {
            if too_long {
                return Ok(Line::TooLong);
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Line::Complete);
        }
    }
}
