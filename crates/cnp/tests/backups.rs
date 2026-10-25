//! Draft backups wire compatibility and typed round trips.
use cnp::{Request, backups, lifecycle};
use serde_json::json;

fn backup(kind: backups::Kind) -> backups::Backup {
    backups::Backup {
        id: "20261025t164012123z-manual".into(),
        kind,
        created_at: "2026-10-25T16:40:12.123Z".into(),
        bytes: 2_457_600,
        games: 1203,
        settings: 14,
    }
}

#[test]
fn the_capability_names_its_contract_and_older_nodes_have_none() {
    let value = json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"LicenseRef-Proprietary"},"capabilities":{"backups":{"list":true,"create":true,"restore":true,"keepAutomatic":7,"contract":"draft-1","future":1}}});
    let result: lifecycle::InitializeResult = serde_json::from_value(value).unwrap();
    let offered = result.capabilities.backups.unwrap();
    assert!(offered.list && offered.create && offered.restore);
    assert_eq!(offered.keep_automatic, Some(7));
    assert_eq!(offered.contract.as_deref(), Some(backups::CONTRACT));
    let older: lifecycle::InitializeResult = serde_json::from_value(
        json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"x"},"capabilities":{"games":{"read":true}}}),
    )
    .unwrap();
    assert!(older.capabilities.backups.is_none());
}

#[test]
fn a_backup_reads_as_the_spec_writes_it() {
    let value = json!({"id":"20261025t164012123z-manual","kind":"manual","createdAt":"2026-10-25T16:40:12.123Z","bytes":2457600,"games":1203,"settings":14,"future":true});
    let read: backups::Backup = serde_json::from_value(value).unwrap();
    assert_eq!(read, backup(backups::Kind::Manual));
    for (kind, text) in [
        (backups::Kind::Automatic, "automatic"),
        (backups::Kind::Manual, "manual"),
        (backups::Kind::BeforeRestore, "beforeRestore"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(text));
    }
    assert!(backups::valid_id("20261025t164012123z-before-restore"));
    for bad in [
        "",
        "Upper",
        "a b",
        "../x",
        &"a".repeat(backups::MAX_ID_BYTES + 1),
    ] {
        assert!(!backups::valid_id(bad), "{bad}");
    }
}

#[test]
fn requests_round_trip_through_their_methods() {
    let list = Request::new::<backups::List>(1, &cnp::NoParams);
    assert_eq!(list.method, "backups/list");
    let listed: backups::ListResult =
        serde_json::from_value(json!({"backups":[backup(backups::Kind::Automatic)]})).unwrap();
    assert_eq!(listed.backups[0].kind, backups::Kind::Automatic);

    // Create's params may be left out, for a backup the player asked for.
    let bare: cnp::Message =
        serde_json::from_str(r#"{"jsonrpc":"2.0","id":2,"method":"backups/create"}"#).unwrap();
    let cnp::Message::Request(bare) = bare else {
        panic!("a request");
    };
    assert_eq!(
        bare.params::<backups::Create>().unwrap(),
        backups::CreateParams { automatic: false }
    );
    let automatic = backups::CreateParams { automatic: true };
    let request = Request::new::<backups::Create>(3, &automatic);
    assert_eq!(request.method, "backups/create");
    assert_eq!(request.params::<backups::Create>().unwrap(), automatic);
    let created: backups::Created =
        serde_json::from_value(json!({"backup":backup(backups::Kind::Automatic),"made":false}))
            .unwrap();
    assert!(!created.made);

    let restore = backups::RestoreParams {
        id: "20261025t164012123z-manual".into(),
    };
    let request = Request::new::<backups::Restore>(4, &restore);
    assert_eq!(request.method, "backups/restore");
    assert_eq!(request.params::<backups::Restore>().unwrap(), restore);
    let restored: backups::Restored = serde_json::from_value(json!({
        "restored": backup(backups::Kind::Manual),
        "before": backup(backups::Kind::BeforeRestore),
    }))
    .unwrap();
    assert_eq!(restored.before.kind, backups::Kind::BeforeRestore);
}
