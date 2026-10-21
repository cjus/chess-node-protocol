//! The envelope: [JSON-RPC 2.0](https://www.jsonrpc.org/specification) messages.
//!
//! Every CNP message is a JSON-RPC request, notification or response (spec, "Messages"):
//!
//! - a **request** has an `id` and a `method`, and is answered by exactly one response with the
//!   same `id`;
//! - a **notification** has a `method` and no `id`, and is never answered;
//! - a **response** has the request's `id` and either a `result` or an `error`.
//!
//! [`Message`] holds any of them with their `params` and `result` as plain JSON, so a program
//! can pass on a message it doesn't understand. The typed helpers ([`Request::new`],
//! [`Request::params`], [`Response::ok`], [`Response::result`] and the notification's) turn
//! them into the protocol's types through the method markers in [`crate::method`].
//!
//! ## Pitfalls
//!
//! - **`"result": null` is a result.** `ping`, `shutdown` and `engine/stop` answer with
//!   `null`, so a response's result can't be an `Option` that `null` turns into `None`.
//!   [`Message::from_value`] tells a missing field from a `null` one.
//! - **A response's `id` can be `null`**, when the request was so broken that its id couldn't
//!   be read (a parse error). Hence [`Response::id`] is an `Option`.
//! - **Batches aren't part of CNP** (as in the Language Server Protocol): an array is an
//!   invalid message.

use std::fmt;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::method::{NotificationMethod, RequestMethod};

/// The JSON-RPC version every message carries in its `jsonrpc` field.
pub const JSONRPC_VERSION: &str = "2.0";

/// The error codes CNP uses: JSON-RPC's own, and the protocol's (spec, "Errors").
pub mod code {
    /// The message isn't JSON.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON isn't a valid message, or the request isn't allowed now (a second
    /// `initialize`).
    pub const INVALID_REQUEST: i64 = -32600;
    /// The node doesn't know the method.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The params are missing, of the wrong shape, or have a wrong value.
    pub const INVALID_PARAMS: i64 = -32602;
    /// Something went wrong inside the node.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// A request other than `initialize` arrived before `initialize`.
    pub const NOT_INITIALIZED: i64 = -32002;
    /// The client cancelled the request (`$/cancelRequest`).
    pub const CANCELLED: i64 = -32800;
    /// The client's protocol version has a major version the node doesn't speak.
    pub const INCOMPATIBLE_VERSION: i64 = -32010;
    /// The position's FEN, or one of its moves, isn't legal.
    pub const INVALID_POSITION: i64 = -32011;
    /// A request goes beyond the limits the node advertised.
    pub const LIMIT_EXCEEDED: i64 = -32012;
    /// The node is already running as many searches as it can.
    pub const BUSY: i64 = -32013;
    /// The request needs a capability the node didn't offer.
    pub const UNSUPPORTED: i64 = -32014;
    /// On the network: no valid credentials.
    pub const UNAUTHORIZED: i64 = -32015;
    /// The node has been asked to shut down and takes no more requests.
    pub const SHUTTING_DOWN: i64 = -32016;
    /// What the request needs isn't ready yet (a voice model still downloading).
    pub const NOT_READY: i64 = -32017;
}

/// A request's id: a number or a string, chosen by whoever sends the request.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    /// A whole number.
    Number(i64),
    /// A string.
    String(String),
}

impl From<i64> for Id {
    fn from(n: i64) -> Id {
        Id::Number(n)
    }
}

impl From<&str> for Id {
    fn from(s: &str) -> Id {
        Id::String(s.to_string())
    }
}

impl From<String> for Id {
    fn from(s: String) -> Id {
        Id::String(s)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Id::Number(n) => write!(f, "{n}"),
            Id::String(s) => write!(f, "{s:?}"),
        }
    }
}

/// A request: a method to call, with its params, answered by a [`Response`] with the same id.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// Identifies the request; its response carries the same id.
    pub id: Id,
    /// The method, like `engine/search`.
    pub method: String,
    /// The params, an object, or `None` when the method takes none.
    pub params: Option<Value>,
}

impl Request {
    /// A request for method `M`, with these params.
    pub fn new<M: RequestMethod>(id: impl Into<Id>, params: &M::Params) -> Request {
        Request {
            id: id.into(),
            method: M::NAME.to_string(),
            params: to_params(params),
        }
    }

    /// Whether this is a request for method `M`.
    pub fn is<M: RequestMethod>(&self) -> bool {
        self.method == M::NAME
    }

    /// The params as method `M`'s type, or an [`code::INVALID_PARAMS`] error to answer with.
    pub fn params<M: RequestMethod>(&self) -> Result<M::Params, ResponseError> {
        from_params(&self.params)
    }
}

/// A notification: a method to call, with its params, never answered.
#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
    /// The method, like `engine/progress`.
    pub method: String,
    /// The params, an object, or `None` when the method takes none.
    pub params: Option<Value>,
}

impl Notification {
    /// A notification of method `M`, with these params.
    pub fn new<M: NotificationMethod>(params: &M::Params) -> Notification {
        Notification {
            method: M::NAME.to_string(),
            params: to_params(params),
        }
    }

    /// Whether this is a notification of method `M`.
    pub fn is<M: NotificationMethod>(&self) -> bool {
        self.method == M::NAME
    }

    /// The params as method `M`'s type.
    pub fn params<M: NotificationMethod>(&self) -> Result<M::Params, ResponseError> {
        from_params(&self.params)
    }
}

/// The answer to a request: its result, or an error.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// The request's id, or `None` (sent as `null`) when it couldn't be read.
    pub id: Option<Id>,
    /// The result, or the error.
    pub outcome: Result<Value, ResponseError>,
}

impl Response {
    /// A successful answer to a request for method `M`.
    pub fn ok<M: RequestMethod>(id: Id, result: &M::Result) -> Response {
        Response {
            id: Some(id),
            outcome: Ok(to_value(result)),
        }
    }

    /// An error answer.
    pub fn error(id: Option<Id>, error: ResponseError) -> Response {
        Response {
            id,
            outcome: Err(error),
        }
    }

    /// The result as method `M`'s type. An error response gives its error back; a result of
    /// the wrong shape gives an [`code::INVALID_PARAMS`] error describing it.
    pub fn result<M: RequestMethod>(&self) -> Result<M::Result, ResponseError> {
        match &self.outcome {
            Ok(value) => serde_json::from_value(value.clone()).map_err(|e| {
                ResponseError::new(
                    code::INVALID_PARAMS,
                    format!("the result of {} isn't valid: {e}", M::NAME),
                )
            }),
            Err(error) => Err(error.clone()),
        }
    }
}

/// An error in a response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponseError {
    /// What kind of error it is: one of [`code`]'s, or the node's own.
    pub code: i64,
    /// A short description for people.
    pub message: String,
    /// More about the error, as the method's spec says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ResponseError {
    /// An error with this code and message, and no data.
    pub fn new(code: i64, message: impl Into<String>) -> ResponseError {
        ResponseError {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// The same error, with this data.
    pub fn with_data(mut self, data: impl Serialize) -> ResponseError {
        self.data = Some(to_value(&data));
        self
    }
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

impl std::error::Error for ResponseError {}

/// Any CNP message.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// A request.
    Request(Request),
    /// A notification.
    Notification(Notification),
    /// A response.
    Response(Response),
}

/// JSON that isn't a valid message, and the id to answer it with if one could be read.
#[derive(Clone, Debug, PartialEq)]
pub struct InvalidMessage {
    /// The request's id, if the message had a readable one.
    pub id: Option<Id>,
    /// What is wrong with it.
    pub reason: String,
}

impl fmt::Display for InvalidMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a valid message: {}", self.reason)
    }
}

impl std::error::Error for InvalidMessage {}

impl Message {
    /// Reads a message from parsed JSON, or says why it isn't one.
    pub fn from_value(value: Value) -> Result<Message, InvalidMessage> {
        let Value::Object(mut object) = value else {
            return Err(invalid(None, "a message is a JSON object"));
        };
        // The id first, so every later error can carry it.
        let id = match object.remove("id") {
            None => None,
            Some(Value::Null) => Some(None),
            Some(Value::Number(n)) => match n.as_i64() {
                Some(n) => Some(Some(Id::Number(n))),
                None => return Err(invalid(None, "a numeric id is a whole number")),
            },
            Some(Value::String(s)) => Some(Some(Id::String(s))),
            Some(_) => return Err(invalid(None, "an id is a number or a string")),
        };
        let known_id = id.clone().flatten();
        if object.get("jsonrpc") != Some(&Value::String(JSONRPC_VERSION.to_string())) {
            return Err(invalid(known_id, "\"jsonrpc\" must be \"2.0\""));
        }
        let params = match object.remove("params") {
            None | Some(Value::Null) => None,
            Some(params @ Value::Object(_)) => Some(params),
            Some(_) => return Err(invalid(known_id, "params are an object")),
        };
        match object.remove("method") {
            Some(Value::String(method)) => match id {
                None => Ok(Message::Notification(Notification { method, params })),
                Some(Some(id)) => Ok(Message::Request(Request { id, method, params })),
                Some(None) => Err(invalid(None, "a request's id can't be null")),
            },
            Some(_) => Err(invalid(known_id, "a method is a string")),
            None => {
                let Some(id) = id else {
                    return Err(invalid(
                        None,
                        "a message has a method, or is a response with an id",
                    ));
                };
                match (object.remove("result"), object.remove("error")) {
                    (Some(result), None) => Ok(Message::Response(Response {
                        id,
                        outcome: Ok(result),
                    })),
                    (None, Some(error)) => match serde_json::from_value(error) {
                        Ok(error) => Ok(Message::Response(Response {
                            id,
                            outcome: Err(error),
                        })),
                        Err(e) => Err(invalid(id, format!("a malformed error: {e}"))),
                    },
                    _ => Err(invalid(id, "a response has either a result or an error")),
                }
            }
        }
    }

    /// The message as JSON.
    pub fn to_value(&self) -> Value {
        to_value(self)
    }
}

impl Serialize for Message {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("jsonrpc", JSONRPC_VERSION)?;
        match self {
            Message::Request(request) => {
                map.serialize_entry("id", &request.id)?;
                map.serialize_entry("method", &request.method)?;
                if let Some(params) = &request.params {
                    map.serialize_entry("params", params)?;
                }
            }
            Message::Notification(notification) => {
                map.serialize_entry("method", &notification.method)?;
                if let Some(params) = &notification.params {
                    map.serialize_entry("params", params)?;
                }
            }
            Message::Response(response) => {
                // Always present, `null` included (see the pitfalls above).
                map.serialize_entry("id", &response.id)?;
                match &response.outcome {
                    Ok(result) => map.serialize_entry("result", result)?,
                    Err(error) => map.serialize_entry("error", error)?,
                }
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Message, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Message::from_value(value).map_err(serde::de::Error::custom)
    }
}

impl FromStr for Message {
    type Err = String;

    /// Parses one message from its JSON text.
    fn from_str(text: &str) -> Result<Message, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Message::from_value(value).map_err(|e| e.to_string())
    }
}

impl From<Request> for Message {
    fn from(request: Request) -> Message {
        Message::Request(request)
    }
}

impl From<Notification> for Message {
    fn from(notification: Notification) -> Message {
        Message::Notification(notification)
    }
}

impl From<Response> for Message {
    fn from(response: Response) -> Message {
        Message::Response(response)
    }
}

fn invalid(id: Option<Id>, reason: impl Into<String>) -> InvalidMessage {
    InvalidMessage {
        id,
        reason: reason.into(),
    }
}

/// Serialises one of the protocol's types. They have only string keys and no custom
/// serialisers that can fail, so this can't fail.
fn to_value(value: &impl Serialize) -> Value {
    serde_json::to_value(value).expect("CNP's types always serialise to JSON")
}

/// Params as sent: a method without params (`()`, which is `null`) sends none.
fn to_params(params: &impl Serialize) -> Option<Value> {
    match to_value(params) {
        Value::Null => None,
        value => Some(value),
    }
}

/// Reads params, or says why they don't fit, as the error to answer with.
fn from_params<T: DeserializeOwned>(params: &Option<Value>) -> Result<T, ResponseError> {
    // A method without params takes `()`, which reads from `null`; one with params needs an
    // object, and `null` fails with a clear message.
    let value = params.clone().unwrap_or(Value::Null);
    serde_json::from_value(value)
        .map_err(|e| ResponseError::new(code::INVALID_PARAMS, format!("invalid params: {e}")))
}
