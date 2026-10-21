//! Reading and writing messages one per line.

use std::io::{BufReader, Cursor};

use cnp::stdio::{ReadError, read_message, write_message};
use cnp::{Message, code};
use serde_json::json;

const MAX: usize = 1024;

fn messages(input: &str) -> Vec<Result<Option<Message>, String>> {
    let mut reader = Cursor::new(input.as_bytes().to_vec());
    let mut out = Vec::new();
    loop {
        match read_message(&mut reader, MAX) {
            Ok(None) => {
                out.push(Ok(None));
                return out;
            }
            Ok(Some(message)) => out.push(Ok(Some(message))),
            Err(e) => out.push(Err(e.to_string())),
        }
    }
}

#[test]
fn messages_are_read_one_per_line_skipping_blank_lines() {
    let read = messages(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n   \r\n\
         {\"jsonrpc\":\"2.0\",\"method\":\"exit\"}\r\n",
    );
    assert_eq!(read.len(), 3);
    assert!(matches!(read[0], Ok(Some(Message::Request(_)))));
    assert!(matches!(read[1], Ok(Some(Message::Notification(_)))));
    assert_eq!(read[2], Ok(None));
}

#[test]
fn a_last_line_without_its_line_break_still_counts() {
    let read = messages("{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}");
    assert!(matches!(read[0], Ok(Some(Message::Notification(_)))));
    assert_eq!(read[1], Ok(None));
}

#[test]
fn a_bad_line_is_reported_and_reading_goes_on() {
    let read = messages("not json\n{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}\n");
    assert!(read[0].as_ref().unwrap_err().starts_with("not JSON"));
    assert!(matches!(read[1], Ok(Some(Message::Notification(_)))));
}

#[test]
fn a_line_over_the_limit_is_skipped_whole() {
    // A long line through a small buffer, so the limit is crossed between reads.
    let long = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"{}\"}}\n",
        "x".repeat(3 * MAX)
    );
    let input = format!("{long}{{\"jsonrpc\":\"2.0\",\"method\":\"exit\"}}\n");
    let mut reader = BufReader::with_capacity(64, Cursor::new(input.into_bytes()));
    assert!(matches!(
        read_message(&mut reader, MAX),
        Err(ReadError::TooLong { max_bytes: MAX })
    ));
    assert!(matches!(
        read_message(&mut reader, MAX),
        Ok(Some(Message::Notification(_)))
    ));
    assert!(matches!(read_message(&mut reader, MAX), Ok(None)));
}

#[test]
fn errors_become_the_responses_a_node_sends() {
    let mut reader = Cursor::new(b"{oops\n{\"jsonrpc\":\"2.0\",\"id\":9}\n".to_vec());
    let parse = read_message(&mut reader, MAX)
        .unwrap_err()
        .response()
        .unwrap();
    assert_eq!(parse.id, None);
    assert_eq!(parse.outcome.unwrap_err().code, code::PARSE_ERROR);
    let invalid = read_message(&mut reader, MAX)
        .unwrap_err()
        .response()
        .unwrap();
    assert_eq!(invalid.id, Some(cnp::Id::Number(9)));
    assert_eq!(invalid.outcome.unwrap_err().code, code::INVALID_REQUEST);
}

#[test]
fn a_written_message_is_one_line_even_with_line_breaks_in_its_text() {
    let message = Message::from_value(json!({"jsonrpc": "2.0", "method": "voice/speak",
        "params": {"text": "one\ntwo"}}))
    .unwrap();
    let mut out = Vec::new();
    write_message(&mut out, &message).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches('\n').count(), 1);
    assert!(text.ends_with('\n'));
    let back = read_message(&mut Cursor::new(text.into_bytes()), MAX)
        .unwrap()
        .unwrap();
    assert_eq!(back, message);
}
