//! Proposed draft-1 preferences extension, for review alongside chessdb's implementation.
//! This module does not change the CNP 1.0 transport or imply deployed client support.
//! See `spec/preferences.md`: sparse typed values, revision checks and durable receipts.
use crate::{NoParams, RequestMethod, lifecycle};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Review identifier. Clients must check this before sending preference operations.
pub const CONTRACT: &str = "draft-1";
/// Solrac's initial sparse preference override schema.
pub const SCHEMA_VERSION: u32 = 1;
/// No mutation: read the current revision and explicitly reconcile before retrying.
pub const REVISION_CONFLICT: i64 = -32020;
/// The operation identity was already used for different contents.
pub const OPERATION_REUSED: i64 = -32021;
/// Storage failed; its commit outcome can be uncertain, so retain the exact request.
pub const STORAGE_FAILURE: i64 = -32022;
/// No new mutation: receipt retention is full; maintenance is required.
pub const CAPACITY: i64 = -32024;

/// Optional preferences domain. Missing switches mean false.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// `preferences/get` is available.
    #[serde(default)]
    pub read: bool,
    /// `preferences/set` is available.
    #[serde(default)]
    pub write: bool,
}
/// Initialize's existing result with optional draft metadata. Existing domains keep
/// using `lifecycle::InitializeResult`; no fields are required of older nodes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Standard CNP version, software, capabilities and limits.
    #[serde(flatten)]
    pub lifecycle: lifecycle::InitializeResult,
    /// The recognized extension contract; absent means do not send preferences requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences_contract: Option<String>,
    /// The application preference schema, independent of SQLite's storage schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences_schema_version: Option<u32>,
}
/// The ordinary initialize exchange, retaining optional preference metadata.
pub enum Initialize {}
impl RequestMethod for Initialize {
    const NAME: &'static str = "initialize";
    type Params = lifecycle::InitializeParams;
    type Result = InitializeResult;
}
/// An explicit type tag prevents string/number/boolean coercions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Setting {
    /// On/off override.
    Boolean(bool),
    /// Unsigned whole-number override.
    Integer(u32),
    /// Finite numeric override; nodes reject non-finite/out-of-schema values.
    Number(f64),
    /// Application-defined text, at most 128 UTF-8 bytes in draft-1.
    Text(String),
}
/// Complete committed overrides. Missing keys select current application defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Version of the application preference schema.
    pub schema_version: u32,
    /// Monotonic revision within this store's history (not across historical restores).
    pub revision: u64,
    /// Names defined by the application schema. Unknown keys must not be silently lost.
    pub values: BTreeMap<String, Setting>,
}
/// Atomic patch. Null removes an override; it is meaningful here, not an omitted value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetParams {
    /// Must match the negotiated application schema.
    pub schema_version: u32,
    /// Unique logical operation identity, retained unchanged through uncertain retries.
    pub operation_id: String,
    /// Revision from the last confirmed read/receipt, checked atomically with the write.
    pub expected_revision: u64,
    /// Nonempty sparse patch; null deletes one override.
    pub changes: BTreeMap<String, Option<Setting>>,
}
/// Read committed overrides. This request never creates application defaults.
pub enum Get {}
impl RequestMethod for Get {
    const NAME: &'static str = "preferences/get";
    type Params = NoParams;
    type Result = Snapshot;
}
/// Commit an atomic patch, or replay the original receipt for an identical operation.
pub enum Set {}
impl RequestMethod for Set {
    const NAME: &'static str = "preferences/set";
    type Params = SetParams;
    type Result = Snapshot;
}
