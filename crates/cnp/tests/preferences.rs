//! Draft preference wire compatibility and typed round trips.
use cnp::{Request, preferences};
use serde_json::json;
#[test]
fn draft_metadata_survives_typed_initialize_and_older_nodes_are_supported() {
    let mut value = json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"LicenseRef-Unspecified"},"capabilities":{"preferences":{"read":true,"write":true}},"limits":{"maxMessageBytes":65536},"preferencesContract":"draft-1","preferencesSchemaVersion":1,"future":true});
    let result: preferences::InitializeResult = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        result.preferences_contract.as_deref(),
        Some(preferences::CONTRACT)
    );
    assert!(result.lifecycle.capabilities.preferences.unwrap().write);
    value.as_object_mut().unwrap().remove("preferencesContract");
    value
        .as_object_mut()
        .unwrap()
        .remove("preferencesSchemaVersion");
    value["capabilities"] = json!({});
    let older: preferences::InitializeResult = serde_json::from_value(value).unwrap();
    assert!(older.preferences_contract.is_none());
    assert!(older.lifecycle.capabilities.preferences.is_none());
}
#[test]
fn patch_preserves_null_and_ignores_unknown_top_level_fields() {
    let params:preferences::SetParams=serde_json::from_value(json!({"schemaVersion":1,"operationId":"op","expectedRevision":3,"changes":{"sound.volume":{"type":"number","value":0.4},"board.labels":null},"future":true})).unwrap();
    let request = Request::new::<preferences::Set>(5, &params);
    assert_eq!(request.params::<preferences::Set>().unwrap(), params);
    assert_eq!(params.changes["board.labels"], None);
    assert!(
        serde_json::from_value::<preferences::Setting>(json!({"type":"boolean","value":"false"}))
            .is_err()
    );
}
