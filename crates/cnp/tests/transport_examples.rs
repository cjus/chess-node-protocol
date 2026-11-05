//! The examples of the network transport in the specification (sections 3.2 and 3.3) are valid
//! JSON of the shapes the text describes, so the spec's examples can't drift from its rules.
//!
//! They are the `transport` blocks of `spec/cnp.md`: one message a line, after `→ ` (from the
//! client) or `← ` (from the node). They aren't JSON-RPC messages (except the `Busy` refusal,
//! which is), so they are not the `cnp` blocks the other example test checks.

use serde_json::Value;

const SPEC: &str = include_str!("../../../spec/cnp.md");

/// The messages of the `transport` blocks: (from the client, the message).
fn messages() -> Vec<(bool, Value)> {
    let mut found = Vec::new();
    let mut inside = false;
    for line in SPEC.lines() {
        match line.trim_end() {
            "```transport" => inside = true,
            "```" => inside = false,
            text if inside => {
                let (client, json) = match (text.strip_prefix("→ "), text.strip_prefix("← ")) {
                    (Some(json), _) => (true, json),
                    (_, Some(json)) => (false, json),
                    _ => panic!("a transport line starts with an arrow: {text}"),
                };
                found.push((client, serde_json::from_str(json).expect("valid JSON")));
            }
            _ => {}
        }
    }
    found
}

#[test]
fn the_transport_examples_are_the_messages_the_text_describes() {
    let all = messages();
    assert_eq!(all.len(), 5, "pair, paired, busy, probe, reply");
    let by = |key: &str, value: &str| {
        all.iter()
            .find(|(_, m)| m[key] == value)
            .unwrap_or_else(|| panic!("no {key}={value} example"))
            .clone()
    };
    let (client, pair) = by("cnp", "pair");
    assert!(client);
    assert_eq!(pair["code"].as_str().unwrap().len(), 8);
    assert!(!by("cnp", "paired").0);
    let (client, probe) = by("cnp", "probe");
    assert!(client && probe["v"] == 1);

    let (client, reply) = by("cnp", "here");
    assert!(!client && reply["v"] == 1);
    assert!(reply["name"].as_str().unwrap().len() <= 32);
    assert!((1..=65535).contains(&reply["port"].as_u64().unwrap()));
    assert!(reply["pairing"].is_boolean());
    // The fingerprint is 32 pairs of uppercase hexadecimal digits with colons: 95 characters.
    let fingerprint = reply["fingerprint"].as_str().unwrap();
    assert_eq!(fingerprint.len(), 95);
    assert!(fingerprint.split(':').count() == 32);
    assert!(fingerprint.split(':').all(|p| {
        p.len() == 2
            && p.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_lowercase())
    }));
    // The reply is never longer than the 320-byte probe it answers.
    assert!(reply.to_string().len() <= 320);

    let busy = all
        .iter()
        .find(|(_, m)| m["error"]["code"] == -32013)
        .expect("the Busy refusal");
    assert!(busy.1["id"].is_null());
}
