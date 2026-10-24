//! # cnp: the Chess Node Protocol
//!
//! The Chess Node Protocol (CNP) connects chess programs to the nodes that serve them: chess
//! engines, and voice modules that speak and listen. It is [JSON-RPC 2.0] over a child
//! process's stdin/stdout, one message per line. The specification is `spec/cnp.md` in this
//! crate's repository; this crate is its types and its connection, for both sides:
//!
//! - [`message`]: the envelope, JSON-RPC's requests, notifications and responses, and the
//!   error [`code`]s;
//! - [`method`]: methods as types, tying each name to its params and result;
//! - [`lifecycle`]: `initialize`, `ping`, `shutdown`, `exit` and `$/cancelRequest`;
//! - [`engine`]: searching positions (`engine/…`);
//! - [`voice`]: text to speech, speech to text, and listening to speech as it comes
//!   (`voice/…`);
//! - [`preferences`] and [`games`]: proposed draft extensions for a node that keeps a
//!   program's settings and its library of games (`preferences/…`, `games/…`);
//! - [`stdio`]: reading and writing messages, one per line.
//!
//! The crate has no runtime and no threads of its own: a program reads messages in a loop of
//! its choosing. A client's first exchange:
//!
//! ```
//! use cnp::lifecycle::{Initialize, InitializeParams, Software, PROTOCOL_VERSION};
//! use cnp::{Message, Request, stdio};
//!
//! let params = InitializeParams {
//!     protocol: PROTOCOL_VERSION,
//!     client: Software { name: "Solrac".into(), version: "0.2.0".into() },
//! };
//! let mut out = Vec::new();
//! stdio::write_message(&mut out, &Request::new::<Initialize>(1, &params).into()).unwrap();
//! assert_eq!(
//!     String::from_utf8(out).unwrap(),
//!     "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\
//!      \"params\":{\"client\":{\"name\":\"Solrac\",\"version\":\"0.2.0\"},\"protocol\":\"1.0\"}}\n"
//! );
//! ```
//!
//! Its design rules (spec, "Design"): chess concepts rather than any engine's internals,
//! requests that stand alone, each side saying what it supports, and questions and answers
//! rather than remote control. The crate depends on nothing but `serde` and `serde_json`, and
//! on no chess library: positions travel as text.
//!
//! [JSON-RPC 2.0]: https://www.jsonrpc.org/specification

pub mod engine;
pub mod games;
pub mod lifecycle;
pub mod message;
pub mod method;
pub mod preferences;
pub mod stdio;
pub mod voice;

pub use message::{
    Id, InvalidMessage, Message, Notification, Request, Response, ResponseError, code,
};
pub use method::{NoParams, NotificationMethod, RequestMethod};
