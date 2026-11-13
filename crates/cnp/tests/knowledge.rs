//! Draft knowledge wire compatibility and typed round trips.
use cnp::{Request, knowledge, lifecycle};
use serde_json::json;

fn manifest() -> serde_json::Value {
    json!({"name":"Lichess puzzles","release":"2026-11","url":"https://database.lichess.org/",
        "retrievedAt":"2026-11-13T10:02:11.000Z","checksum":"a".repeat(64),
        "license":"CC0-1.0","attribution":"Lichess"})
}

fn params() -> knowledge::ImportParams {
    serde_json::from_value(json!({"kind":"lichess-puzzles",
        "path":"/Users/carlos/Downloads/lichess_db_puzzle.csv.zst","manifest":manifest()}))
    .unwrap()
}

#[test]
fn the_capability_names_its_kinds_and_contract_and_older_nodes_have_none() {
    let value = json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"x"},
        "capabilities":{"knowledge":{"import":true,"kinds":["lichess-puzzles","sts-epd"],
        "contract":"draft-1","future":1}}});
    let result: lifecycle::InitializeResult = serde_json::from_value(value).unwrap();
    let offered = result.capabilities.knowledge.unwrap();
    assert!(offered.import);
    assert_eq!(offered.kinds, ["lichess-puzzles", "sts-epd"]);
    assert_eq!(offered.contract.as_deref(), Some(knowledge::CONTRACT));
    let older: lifecycle::InitializeResult = serde_json::from_value(json!({"protocol":"1.0",
        "node":{"name":"chessdb","version":"0.1","license":"x"},"capabilities":{"backups":{}}}))
    .unwrap();
    assert!(older.capabilities.knowledge.is_none());
}

#[test]
fn an_import_request_reads_as_the_spec_writes_it() {
    let p = params();
    assert!(p.validate().is_ok());
    let request = Request::new::<knowledge::StartImport>(1, &p);
    assert_eq!(request.method, "knowledge/import");
    assert_eq!(request.params::<knowledge::StartImport>().unwrap(), p);
    assert_eq!(serde_json::to_value(&p.manifest).unwrap(), manifest());
}

#[test]
fn params_with_a_bad_shape_say_what_is_wrong() {
    let mut p = params();
    p.path = "Downloads/x.zst".into();
    assert!(p.validate().unwrap_err().contains("absolute"));
    for ok in ["/a", "C:\\Users\\x", "D:/x", "\\\\server\\share\\x"] {
        p.path = ok.into();
        assert!(p.validate().is_ok(), "{ok}");
    }
    for bad in [
        "",
        "a\0b",
        "relative/x",
        &format!("/{}", "x".repeat(knowledge::MAX_PATH_BYTES)),
    ] {
        p.path = bad.into();
        assert!(p.validate().is_err(), "{bad:?}");
    }
    p = params();
    p.kind = "Lichess Puzzles".into();
    assert!(p.validate().is_err());
    for edit in [
        |m: &mut knowledge::Manifest| m.checksum = "abc".into(),
        |m: &mut knowledge::Manifest| m.checksum = "z".repeat(64),
        |m: &mut knowledge::Manifest| m.license = " ".repeat(0),
        |m: &mut knowledge::Manifest| m.attribution = "a\nb".into(),
        |m: &mut knowledge::Manifest| m.url = "u".repeat(knowledge::MAX_FIELD_BYTES + 1),
    ] {
        p = params();
        edit(&mut p.manifest);
        assert!(p.validate().is_err());
    }
    // Either case of hex is a checksum.
    assert!(knowledge::valid_checksum(&"Ab".repeat(32)));
}

#[test]
fn an_import_reads_and_writes_with_its_state_and_failure() {
    let value = json!({"kind":"lichess-puzzles","checksum":"a".repeat(64),"name":"Lichess puzzles",
        "release":"2026-11","state":"importing","read":120000,"accepted":119871,"rejected":3,
        "duplicate":126,"startedAt":"2026-11-13T10:05:00.000Z","future":true});
    let import: knowledge::Import = serde_json::from_value(value).unwrap();
    assert_eq!(import.state, knowledge::State::Importing);
    assert_eq!(
        import.read,
        import.accepted + import.rejected + import.duplicate
    );
    assert!(import.failure.is_none() && import.finished_at.is_none());
    let written = serde_json::to_value(&import).unwrap();
    assert!(written.get("failure").is_none() && written.get("finishedAt").is_none());
    for (state, text) in [
        (knowledge::State::Verifying, "verifying"),
        (knowledge::State::Importing, "importing"),
        (knowledge::State::Complete, "complete"),
        (knowledge::State::Cancelled, "cancelled"),
        (knowledge::State::Failed, "failed"),
    ] {
        assert_eq!(serde_json::to_value(state).unwrap(), json!(text));
    }
    let failed: knowledge::Import = serde_json::from_value(json!({"kind":"sts-epd",
        "checksum":"b".repeat(64),"name":"STS","release":"v3","state":"failed","read":0,
        "accepted":0,"rejected":0,"duplicate":0,
        "failure":{"code":"checksum","message":"not the file the manifest describes"},
        "startedAt":"2026-11-13T10:05:00.000Z"}))
    .unwrap();
    assert_eq!(
        failed.failure.unwrap().code,
        knowledge::FailureCode::Checksum
    );
}

#[test]
fn status_params_may_be_left_out_and_cancel_names_an_identity() {
    let none = Request::new::<knowledge::ListImports>(2, &knowledge::StatusParams::default());
    assert_eq!(none.method, "knowledge/status");
    let from_nothing: knowledge::StatusParams = serde_json::from_value(json!(null)).unwrap();
    assert_eq!(from_nothing, knowledge::StatusParams::default());
    let one: knowledge::StatusParams =
        serde_json::from_value(json!({"kind":"sts-epd","checksum":"b".repeat(64)})).unwrap();
    assert_eq!(one.kind.as_deref(), Some("sts-epd"));
    let cancel = Request::new::<knowledge::CancelImport>(
        3,
        &knowledge::CancelParams {
            kind: "sts-epd".into(),
            checksum: "b".repeat(64),
        },
    );
    assert_eq!(cancel.method, "knowledge/cancel");
    let bad: Result<knowledge::CancelParams, _> = serde_json::from_value(json!({"kind":"x"}));
    assert!(bad.is_err());
}

#[test]
fn the_specs_json_examples_are_valid() {
    const SPEC: &str = include_str!("../../../spec/knowledge.md");
    let mut blocks = Vec::new();
    let mut open: Option<String> = None;
    for line in SPEC.lines() {
        match (&mut open, line.trim_end()) {
            (None, "```json") => open = Some(String::new()),
            (Some(_), "```") => blocks.push(open.take().unwrap()),
            (Some(text), l) => {
                text.push_str(l);
                text.push('\n');
            }
            _ => {}
        }
    }
    // Blocks with `…` are abridged on purpose (a checksum); fill them in before parsing.
    let full = "a".repeat(64);
    let mut checked = 0;
    for block in blocks {
        let text = block
            .replace("3f1c0d…64 hex digits…9a", &full)
            .replace("3f1c0d…9a", &full)
            .replace("{ ... a manifest ... }", &manifest().to_string())
            .replace("{ ... an import, verifying ... }", "{\"kind\":\"k\",\"checksum\":\"a\",\"name\":\"n\",\"release\":\"r\",\"state\":\"verifying\",\"read\":0,\"accepted\":0,\"rejected\":0,\"duplicate\":0,\"startedAt\":\"2026-11-13T10:05:00.000Z\"}")
            .replace("{ ... an import ... }", "{\"kind\":\"k\",\"checksum\":\"a\",\"name\":\"n\",\"release\":\"r\",\"state\":\"complete\",\"read\":0,\"accepted\":0,\"rejected\":0,\"duplicate\":0,\"startedAt\":\"2026-11-13T10:05:00.000Z\"}");
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
        if let Some(c) = value.pointer("/capabilities/knowledge") {
            let c: knowledge::Capabilities = serde_json::from_value(c.clone()).unwrap();
            assert_eq!(c.contract.as_deref(), Some(knowledge::CONTRACT));
        } else if value.get("manifest").is_some() {
            serde_json::from_value::<knowledge::ImportParams>(value)
                .unwrap()
                .validate()
                .unwrap();
        } else if value.get("import").is_some() {
            serde_json::from_value::<knowledge::ImportResult>(value).unwrap();
        } else if value.get("imports").is_some() {
            serde_json::from_value::<knowledge::StatusResult>(value).unwrap();
        } else if value.get("license").is_some() {
            serde_json::from_value::<knowledge::Manifest>(value)
                .unwrap()
                .validate()
                .unwrap();
        } else if value.get("state").is_some() {
            serde_json::from_value::<knowledge::Import>(value).unwrap();
        } else if value.get("path").is_none() {
            serde_json::from_value::<knowledge::StatusParams>(value).unwrap();
        }
        checked += 1;
    }
    assert!(checked >= 7, "only {checked} examples");
}
