//! Methods as types: each one's name and the types of its params and result.
//!
//! A method is an empty enum that implements [`RequestMethod`] or [`NotificationMethod`], as
//! in the `lsp-types` crate. The marker ties the name to its types, so typed helpers like
//! [`Request::new`](crate::Request::new) can't send `engine/search` with the params of
//! `voice/speak`:
//!
//! ```
//! use cnp::{Request, engine};
//!
//! let params = engine::SearchParams::new(engine::Position::start());
//! let request = Request::new::<engine::Search>(7, &params);
//! assert_eq!(request.method, "engine/search");
//! assert_eq!(request.params::<engine::Search>().unwrap(), params);
//! ```

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A method called with a request, and answered.
pub trait RequestMethod {
    /// The method's name, like `engine/search`.
    const NAME: &'static str;
    /// Its params. [`NoParams`] for a method without params.
    type Params: Serialize + DeserializeOwned;
    /// Its result. `()` for a method that answers `null`.
    type Result: Serialize + DeserializeOwned;
}

/// A method called with a notification, never answered.
pub trait NotificationMethod {
    /// The method's name, like `engine/progress`.
    const NAME: &'static str;
    /// Its params. [`NoParams`] for a method without params.
    type Params: Serialize + DeserializeOwned;
}

/// The params of a method that takes none.
///
/// It is sent as nothing (no `params` field), and read from nothing, `null` or any object,
/// whose fields are ignored like any unknown field (spec, 2). `()` would refuse `{}`, which a
/// client may well send.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoParams;

impl Serialize for NoParams {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for NoParams {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<NoParams, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::Null | serde_json::Value::Object(_) => Ok(NoParams),
            other => Err(serde::de::Error::custom(format!(
                "this method takes no params, got {other}"
            ))),
        }
    }
}
