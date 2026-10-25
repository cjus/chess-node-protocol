//! Starting, checking and ending a connection (spec, "Lifecycle"), and cancelling requests.
//!
//! ```text
//!   client                                   node
//!   initialize {protocol, client} ────────▶
//!                 ◀──────── {protocol, node, capabilities, limits}
//!   … the domains' requests, ping at any time …
//!   shutdown ─────────────────────────────▶  ends its work
//!                 ◀──────── null
//!   exit ─────────────────────────────────▶  ends, with code 0
//! ```
//!
//! `initialize` comes first and settles the protocol version both sides speak
//! ([`negotiate`]): the client's major version, and the lower of the two minor versions. A
//! node answers anything else that comes before it with [`code::NOT_INITIALIZED`].
//!
//! [`code::NOT_INITIALIZED`]: crate::code::NOT_INITIALIZED

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::engine::EngineCapabilities;
use crate::message::Id;
use crate::method::{NoParams, NotificationMethod, RequestMethod};
use crate::voice::VoiceCapabilities;

/// The version of the protocol this crate implements.
pub const PROTOCOL_VERSION: Version = Version { major: 1, minor: 0 };

/// The longest message a node takes unless it advertises otherwise: 16 MiB, room for about a
/// minute of speech as base64.
pub const DEFAULT_MAX_MESSAGE_BYTES: u64 = 16 * 1024 * 1024;

/// A protocol version, written `major.minor` (`"1.0"`).
///
/// A new minor version only adds: optional fields, methods and capabilities, which older
/// programs ignore or never see offered. A new major version may change anything, and two
/// programs with different major versions don't talk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// The major version: programs talk only if theirs are equal.
    pub major: u32,
    /// The minor version.
    pub minor: u32,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(text: &str) -> Result<Version, String> {
        let bad = || format!("{text:?} isn't a version like \"1.0\"");
        let (major, minor) = text.split_once('.').ok_or_else(bad)?;
        Ok(Version {
            major: major.parse().map_err(|_| bad())?,
            minor: minor.parse().map_err(|_| bad())?,
        })
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Version, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The version a client and a node speak: the client's major version if the node speaks it,
/// with the lower of their minor versions. `None` if their major versions differ.
pub fn negotiate(client: Version, node: Version) -> Option<Version> {
    (client.major == node.major).then(|| Version {
        major: node.major,
        minor: client.minor.min(node.minor),
    })
}

/// The `initialize` request: the first of every connection.
pub enum Initialize {}

impl RequestMethod for Initialize {
    const NAME: &'static str = "initialize";
    type Params = InitializeParams;
    type Result = InitializeResult;
}

/// What the client sends with `initialize`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// The newest protocol version the client speaks.
    pub protocol: Version,
    /// The client program.
    pub client: Software,
}

/// A program's name and version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Software {
    /// Its name, like `Solrac`.
    pub name: String,
    /// Its version, like `0.2.0`.
    pub version: String,
}

/// What the node answers to `initialize`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// The version both sides speak from now on ([`negotiate`]).
    pub protocol: Version,
    /// The node program.
    pub node: NodeInfo,
    /// What the node can do.
    pub capabilities: Capabilities,
    /// How far it will go.
    #[serde(default)]
    pub limits: Limits,
}

/// The node program: what it is, and where its source is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    /// Its name, like `cb2026`.
    pub name: String,
    /// Its version.
    pub version: String,
    /// Who made it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Its licence, as an SPDX expression like `MIT OR Apache-2.0`, so a client can show it.
    pub license: String,
    /// Where its source is, a URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The domains a node serves, each with its own capabilities. A domain that is missing isn't
/// served: its requests are answered with [`code::METHOD_NOT_FOUND`](crate::code::METHOD_NOT_FOUND).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Proposed draft preferences domain; clients must also check its initialize metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<crate::preferences::Capabilities>,
    /// Proposed draft games domain (`games/…`); clients must also check its contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub games: Option<crate::games::Capabilities>,
    /// Proposed draft backups domain (`backups/…`); clients must also check its contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backups: Option<crate::backups::Capabilities>,
    /// The engine domain (`engine/…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineCapabilities>,
    /// The voice domain (`voice/…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceCapabilities>,
}

/// How far a node will go. A request beyond a limit is refused with
/// [`code::LIMIT_EXCEEDED`](crate::code::LIMIT_EXCEEDED), never trimmed silently. A missing
/// limit is the default the spec gives, or none.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// The deepest search, in plies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<u32>,
    /// The most nodes one search may visit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<u64>,
    /// The longest one search may take, in milliseconds. It bounds infinite searches too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_ms: Option<u64>,
    /// The most lines one search may return (MultiPV). 1 if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_multipv: Option<u32>,
    /// How many searches may run at once. 1 if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_searches: Option<u32>,
    /// The longest message the node reads, in bytes. [`DEFAULT_MAX_MESSAGE_BYTES`] if missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_message_bytes: Option<u64>,
}

impl Limits {
    /// The most lines one search may return.
    pub fn multipv(&self) -> u32 {
        self.max_multipv.unwrap_or(1)
    }

    /// How many searches may run at once.
    pub fn concurrent_searches(&self) -> u32 {
        self.max_concurrent_searches.unwrap_or(1)
    }

    /// The longest message the node reads.
    pub fn message_bytes(&self) -> u64 {
        self.max_message_bytes.unwrap_or(DEFAULT_MAX_MESSAGE_BYTES)
    }
}

/// The `data` of an [`code::INCOMPATIBLE_VERSION`](crate::code::INCOMPATIBLE_VERSION) error:
/// the versions the node speaks, so the client can say what to upgrade.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompatibleVersionData {
    /// The newest version of each major version the node speaks.
    pub supported: Vec<Version>,
}

/// The `ping` request: answered with `null` at once, at any time after `initialize`, even
/// while the node searches, so a client can tell a busy node from a dead one.
pub enum Ping {}

impl RequestMethod for Ping {
    const NAME: &'static str = "ping";
    type Params = NoParams;
    type Result = ();
}

/// The `shutdown` request: the node cancels its running work, answers `null`, and answers any
/// later request with [`code::SHUTTING_DOWN`](crate::code::SHUTTING_DOWN).
pub enum Shutdown {}

impl RequestMethod for Shutdown {
    const NAME: &'static str = "shutdown";
    type Params = NoParams;
    type Result = ();
}

/// The `exit` notification: the node ends, with exit code 0 if `shutdown` came first and 1
/// otherwise.
pub enum Exit {}

impl NotificationMethod for Exit {
    const NAME: &'static str = "exit";
    type Params = NoParams;
}

/// The `$/cancelRequest` notification: drop a request. Unless it has already been answered,
/// the request is answered at once with [`code::CANCELLED`](crate::code::CANCELLED), and
/// nothing it did counts.
pub enum CancelRequest {}

impl NotificationMethod for CancelRequest {
    const NAME: &'static str = "$/cancelRequest";
    type Params = CancelParams;
}

/// Which request to cancel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelParams {
    /// The request's id.
    pub id: Id,
}
