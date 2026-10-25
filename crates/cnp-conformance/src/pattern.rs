//! Patterns: what a session expects a node to send.
//!
//! A pattern is JSON that a message must match:
//!
//! - **An object** matches an object that has every key the pattern has, each matching. Keys
//!   the pattern doesn't name may be there or not, as unknown fields are ignored (spec, 2).
//! - **An array** matches an array of the same length, element by element.
//! - **A number, a boolean or `null`** matches the same value. Numbers compare as numbers, so
//!   `1` matches `1.0`.
//! - **A string** matches the same string, unless it starts with `$`:
//!
//! | Pattern | Matches |
//! | --- | --- |
//! | `"$any"` | anything present |
//! | `"$absent"` | a missing field (or `null`, which means the same) |
//! | `"$string"`, `"$number"`, `"$integer"`, `"$boolean"`, `"$array"`, `"$object"`, `"$null"` | any value of that type |
//! | `"$move"` | a string in coordinate notation, like `e2e4` or `e7e8q` |
//! | `"$bind:NAME"` | anything, and remembers it as `NAME` for the rest of the session |
//! | `"$ref:NAME"` | the value remembered as `NAME` |
//! | `"$$text"` | the string `$text` |

use std::collections::HashMap;
use std::fmt;

use serde_json::Value;

/// Values remembered with `$bind:NAME`, for `$ref:NAME`.
pub type Bindings = HashMap<String, Value>;

/// `message` to send, with each string `"$ref:NAME"` in it replaced by the value bound as
/// `NAME` ([`Bindings`]), so a step can send what an earlier answer said, such as an
/// identity the node chose. No other string changes. An unbound name is an error.
pub fn fill(message: &Value, bindings: &Bindings) -> Result<Value, String> {
    Ok(match message {
        Value::String(text) => match text.strip_prefix("$ref:") {
            Some(name) => bindings
                .get(name)
                .cloned()
                .ok_or_else(|| format!("$ref:{name} sent before anything was bound as {name}"))?,
            None => message.clone(),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| fill(item, bindings))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| Ok((key.clone(), fill(value, bindings)?)))
                .collect::<Result<_, String>>()?,
        ),
        _ => message.clone(),
    })
}

/// Why a message doesn't match: where, and what was wrong there.
#[derive(Clone, Debug, PartialEq)]
pub struct Mismatch {
    /// The path to the value, like `result.score.cp`.
    pub path: String,
    /// What was wrong.
    pub reason: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.reason)
        } else {
            write!(f, "at {}: {}", self.path, self.reason)
        }
    }
}

/// Whether `actual` matches `pattern`. The bindings change only if it does.
pub fn matches(pattern: &Value, actual: &Value, bindings: &mut Bindings) -> Result<(), Mismatch> {
    let mut trial = bindings.clone();
    check(pattern, Some(actual), "", &mut trial)?;
    *bindings = trial;
    Ok(())
}

fn check(
    pattern: &Value,
    actual: Option<&Value>,
    path: &str,
    bindings: &mut Bindings,
) -> Result<(), Mismatch> {
    let fail = |reason: String| {
        Err(Mismatch {
            path: path.to_string(),
            reason,
        })
    };
    if let Value::String(text) = pattern
        && let Some(special) = text.strip_prefix('$')
        && !special.starts_with('$')
    {
        return check_special(special, actual, path, bindings);
    }
    let Some(actual) = actual else {
        return fail(format!("missing, expected {pattern}"));
    };
    match (pattern, actual) {
        (Value::Object(expected), Value::Object(object)) => {
            for (key, value) in expected {
                let inner = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                check(value, object.get(key), &inner, bindings)?;
            }
            Ok(())
        }
        (Value::Array(expected), Value::Array(array)) => {
            if expected.len() != array.len() {
                return fail(format!(
                    "{} elements, expected {}",
                    array.len(),
                    expected.len()
                ));
            }
            for (i, (p, a)) in expected.iter().zip(array).enumerate() {
                check(p, Some(a), &format!("{path}[{i}]"), bindings)?;
            }
            Ok(())
        }
        (Value::Number(p), Value::Number(a)) => {
            let equal = match (p.as_i64(), a.as_i64()) {
                (Some(p), Some(a)) => p == a,
                _ => p.as_f64() == a.as_f64(),
            };
            if equal {
                Ok(())
            } else {
                fail(format!("{actual}, expected {pattern}"))
            }
        }
        (Value::String(p), Value::String(a)) => {
            // `$$text` stands for `$text`.
            let p = p
                .strip_prefix('$')
                .filter(|rest| rest.starts_with('$'))
                .unwrap_or(p);
            if p == a {
                Ok(())
            } else {
                fail(format!("{actual}, expected {p:?}"))
            }
        }
        (p, a) if p == a => Ok(()),
        _ => fail(format!("{actual}, expected {pattern}")),
    }
}

fn check_special(
    special: &str,
    actual: Option<&Value>,
    path: &str,
    bindings: &mut Bindings,
) -> Result<(), Mismatch> {
    let fail = |reason: String| {
        Err(Mismatch {
            path: path.to_string(),
            reason,
        })
    };
    if special == "absent" {
        return match actual {
            None | Some(Value::Null) => Ok(()),
            Some(value) => fail(format!("{value}, expected nothing")),
        };
    }
    let Some(actual) = actual else {
        return fail(format!("missing, expected ${special}"));
    };
    let ok = match special {
        "any" => true,
        "string" => actual.is_string(),
        "number" => actual.is_number(),
        "integer" => actual.is_i64() || actual.is_u64(),
        "boolean" => actual.is_boolean(),
        "array" => actual.is_array(),
        "object" => actual.is_object(),
        "null" => actual.is_null(),
        "move" => actual.as_str().is_some_and(cnp::engine::is_move_text),
        _ => {
            if let Some(name) = special.strip_prefix("bind:") {
                bindings.insert(name.to_string(), actual.clone());
                return Ok(());
            }
            if let Some(name) = special.strip_prefix("ref:") {
                return match bindings.get(name) {
                    Some(bound) if bound == actual => Ok(()),
                    Some(bound) => fail(format!("{actual}, expected {bound} (${name})")),
                    None => fail(format!("${name} was never bound")),
                };
            }
            return fail(format!("unknown pattern ${special}"));
        }
    };
    if ok {
        Ok(())
    } else {
        fail(format!("{actual}, expected ${special}"))
    }
}
