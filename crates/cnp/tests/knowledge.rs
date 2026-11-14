//! Draft knowledge wire compatibility and typed round trips.
use cnp::{Request, knowledge, lifecycle};
use serde_json::json;

fn manifest() -> serde_json::Value {
    json!({"name":"Lichess puzzles","release":"2026-11","url":"https://database.lichess.org/",
        "retrievedAt":"2026-11-13T10:02:11.000Z","checksum":"a".repeat(64),
        "license":"CC0-1.0","attribution":"Lichess"})
}

fn source() -> serde_json::Value {
    json!({"checksum":"a".repeat(64),"kind":"lichess-puzzles","name":"Lichess puzzles",
        "release":"2026-11","license":"CC0-1.0","attribution":"Lichess"})
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
            .replace("{ ... a source ... }", &source().to_string())
            .replace("\"next\": \"…\"", "\"next\": \"x\"")
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
        } else if value.get("puzzles").is_some() {
            let r: knowledge::PuzzlesResult = serde_json::from_value(value).unwrap();
            assert_eq!(r.puzzles[0].moves.len(), 6);
        } else if value.get("analyses").is_some() {
            let r: knowledge::AnalysesResult = serde_json::from_value(value).unwrap();
            assert_eq!(
                r.analyses[0].perspective,
                knowledge::Perspective::Unspecified
            );
        } else if value.get("exercises").is_some() {
            let r: knowledge::ExercisesResult = serde_json::from_value(value).unwrap();
            assert_eq!(r.exercises[0].candidates[0].uci, "f4f5");
        } else if value.get("minRating").is_some() || value.get("daily").is_some() {
            serde_json::from_value::<knowledge::PuzzlesParams>(value)
                .unwrap()
                .validate()
                .unwrap();
        } else if value.get("bands").is_some() {
            let r: knowledge::FacetsResult = serde_json::from_value(value).unwrap();
            // One count for each band, for the total and for every theme.
            assert_eq!(r.total.len(), r.bands.len());
            assert!(r.themes.iter().all(|t| t.counts.len() == r.bands.len()));
            assert_eq!(r.bands.last().unwrap().max_rating, None);
            assert!(
                r.bands
                    .windows(2)
                    .all(|w| w[0].max_rating == Some(w[1].min_rating - 1))
            );
        } else if value.get("quality").is_some() {
            serde_json::from_value::<knowledge::FacetsParams>(value).unwrap();
        } else if value.get("fen").is_some() {
            serde_json::from_value::<knowledge::AnalysesParams>(value)
                .unwrap()
                .validate()
                .unwrap();
        } else if value.get("checksum").is_some()
            && value.get("kind").is_some()
            && value.get("attribution").is_some()
            && value.get("state").is_none()
        {
            serde_json::from_value::<knowledge::Source>(value).unwrap();
        } else if value.get("license").is_some() {
            serde_json::from_value::<knowledge::Manifest>(value)
                .unwrap()
                .validate()
                .unwrap();
        } else if value.get("state").is_some() {
            serde_json::from_value::<knowledge::Import>(value).unwrap();
        } else if value.get("theme").is_some() {
            serde_json::from_value::<knowledge::ExercisesParams>(value)
                .unwrap()
                .validate()
                .unwrap();
        } else if value.get("path").is_none() {
            serde_json::from_value::<knowledge::StatusParams>(value).unwrap();
        }
        checked += 1;
    }
    assert!(checked >= 14, "only {checked} examples");
}

#[test]
fn the_read_capability_is_separate_from_import() {
    let c: knowledge::Capabilities =
        serde_json::from_value(json!({"import":true,"contract":"draft-1"})).unwrap();
    assert!(c.import && !c.read);
    let c: knowledge::Capabilities =
        serde_json::from_value(json!({"read":true,"kinds":["sts-epd"],"contract":"draft-1"}))
            .unwrap();
    assert!(c.read && !c.import);
}

#[test]
fn read_params_may_be_left_out_are_checked_and_skip_what_isnt_set() {
    let none: knowledge::PuzzlesParams = serde_json::from_value(json!(null)).unwrap();
    assert_eq!(none, knowledge::PuzzlesParams::default());
    assert!(none.validate().is_ok());
    assert_eq!(serde_json::to_value(&none).unwrap(), json!({}));
    let p: knowledge::PuzzlesParams = serde_json::from_value(
        json!({"theme":"mateIn1","minRating":1000,"maxRating":1400,"limit":10,"future":1}),
    )
    .unwrap();
    assert_eq!(
        (p.min_rating, p.max_rating, p.limit),
        (Some(1000), Some(1400), Some(10))
    );
    let request = Request::new::<knowledge::FindPuzzles>(1, &p);
    assert_eq!(request.method, "knowledge/puzzles");
    assert_eq!(request.params::<knowledge::FindPuzzles>().unwrap(), p);
    for bad in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"theme":"mate in 1"}),
        json!({"theme":""}),
        json!({"minRating":1500,"maxRating":1000}),
        json!({"after":""}),
        json!({"after":"x".repeat(knowledge::MAX_CURSOR_BYTES + 1)}),
    ] {
        let params: knowledge::PuzzlesParams = serde_json::from_value(bad.clone()).unwrap();
        assert!(params.validate().is_err(), "{bad}");
    }
    let e: knowledge::ExercisesParams = serde_json::from_value(json!(null)).unwrap();
    assert!(e.validate().is_ok());
    assert!(
        knowledge::ExercisesParams {
            limit: Some(0),
            ..e
        }
        .validate()
        .is_err()
    );
}

#[test]
fn analyses_params_need_a_fen_of_four_or_six_fields() {
    let ok = |fen: &str| {
        knowledge::AnalysesParams {
            fen: fen.into(),
            limit: None,
        }
        .validate()
    };
    assert!(ok("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -").is_ok());
    assert!(ok("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").is_ok());
    assert!(ok("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq").is_err());
    assert!(ok("").is_err());
    assert!(ok(&"x ".repeat(100)).is_err());
    assert!(
        knowledge::AnalysesParams {
            fen: "a b c d".into(),
            limit: Some(0)
        }
        .validate()
        .is_err()
    );
    let missing: Result<knowledge::AnalysesParams, _> = serde_json::from_value(json!({"limit":5}));
    assert!(missing.is_err());
}

#[test]
fn perspectives_and_candidates_are_written_as_the_spec_has_them() {
    for (p, text) in [
        (knowledge::Perspective::White, "white"),
        (knowledge::Perspective::SideToMove, "sideToMove"),
        (knowledge::Perspective::Unspecified, "unspecified"),
    ] {
        assert_eq!(serde_json::to_value(p).unwrap(), json!(text));
    }
    let c = knowledge::Candidate {
        uci: "e2e4".into(),
        points: 10,
    };
    assert_eq!(
        serde_json::to_value(&c).unwrap(),
        json!({"move":"e2e4","points":10})
    );
    // A mate line has no cp; a centipawn line has no mate.
    let line: knowledge::Line = serde_json::from_value(json!({"mate":1,"moves":"h5f7"})).unwrap();
    assert_eq!((line.cp, line.mate), (None, Some(1)));
    assert_eq!(
        serde_json::to_value(&line).unwrap(),
        json!({"mate":1,"moves":"h5f7"})
    );
}

#[test]
fn the_selection_capability_is_its_own_switch_and_older_nodes_lack_it() {
    let c: knowledge::Capabilities =
        serde_json::from_value(json!({"read":true,"contract":"draft-1"})).unwrap();
    assert!(c.read && !c.puzzle_selection);
    let c: knowledge::Capabilities =
        serde_json::from_value(json!({"read":true,"puzzleSelection":true,"contract":"draft-1"}))
            .unwrap();
    assert!(c.puzzle_selection);
    assert_eq!(
        serde_json::to_value(&c).unwrap()["puzzleSelection"],
        json!(true)
    );
}

#[test]
fn order_seed_and_quality_are_written_as_the_spec_has_them() {
    let p: knowledge::PuzzlesParams = serde_json::from_value(
        json!({"theme":"fork","quality":"reliable","order":"random","seed":7,"limit":1}),
    )
    .unwrap();
    assert_eq!(
        (p.order, p.seed, p.quality),
        (
            Some(knowledge::Order::Random),
            Some(7),
            Some(knowledge::Quality::Reliable)
        )
    );
    assert!(p.validate().is_ok());
    // What was not asked for is not written, so an older node sees nothing new.
    let plain = knowledge::PuzzlesParams {
        theme: Some("fork".into()),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        json!({"theme":"fork"})
    );
    let largest: knowledge::PuzzlesParams =
        serde_json::from_value(json!({"order":"random","seed":4294967295u32})).unwrap();
    assert!(largest.validate().is_ok());
}

#[test]
fn a_seed_needs_random_order_and_unknown_values_are_refused() {
    for bad in [json!({"seed":3}), json!({"order":"easiest","seed":3})] {
        let p: knowledge::PuzzlesParams = serde_json::from_value(bad.clone()).unwrap();
        assert!(p.validate().unwrap_err().contains("seed"), "{bad}");
    }
    // An unknown order or quality is not read at all, which the node answers as invalid params.
    for bad in [
        json!({"order":"shuffled"}),
        json!({"order":"Random"}),
        json!({"quality":"best"}),
        json!({"seed":-1,"order":"random"}),
        json!({"seed":4294967296u64,"order":"random"}),
    ] {
        assert!(
            serde_json::from_value::<knowledge::PuzzlesParams>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_puzzles_answer_names_its_seed_and_thresholds_only_when_there_are_any() {
    let plain: knowledge::PuzzlesResult =
        serde_json::from_value(json!({"puzzles":[],"sources":[]})).unwrap();
    assert_eq!((plain.seed, plain.quality), (None, None));
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        json!({"puzzles":[],"sources":[]})
    );
    let asked: knowledge::PuzzlesResult = serde_json::from_value(json!({"puzzles":[],"sources":[],
        "seed":7,"quality":{"minPlays":100,"minPopularity":80,"maxRatingDeviation":110}}))
    .unwrap();
    assert_eq!(asked.seed, Some(7));
    assert_eq!(asked.quality.unwrap().min_plays, 100);
}

#[test]
fn facets_params_may_be_left_out_and_the_answer_is_counts_by_band() {
    let none: knowledge::FacetsParams = serde_json::from_value(json!(null)).unwrap();
    assert_eq!(none, knowledge::FacetsParams::default());
    assert_eq!(serde_json::to_value(&none).unwrap(), json!({}));
    let request = Request::new::<knowledge::FindPuzzleFacets>(
        1,
        &knowledge::FacetsParams {
            quality: Some(knowledge::Quality::Reliable),
        },
    );
    assert_eq!(request.method, "knowledge/puzzleFacets");
    assert_eq!(
        request
            .params::<knowledge::FindPuzzleFacets>()
            .unwrap()
            .quality,
        Some(knowledge::Quality::Reliable)
    );
    let answer: knowledge::FacetsResult = serde_json::from_value(json!({
        "bands":[{"minRating":0,"maxRating":999},{"minRating":1000}],
        "total":[5,7],
        "themes":[{"theme":"fork","counts":[2,3]}],
        "sources":[]}))
    .unwrap();
    assert_eq!(answer.bands[1].max_rating, None);
    assert_eq!(answer.themes[0].counts, [2, 3]);
    assert_eq!(
        serde_json::to_value(answer.bands[0]).unwrap(),
        json!({"minRating":0,"maxRating":999})
    );
    assert_eq!(
        serde_json::to_value(answer.bands[1]).unwrap(),
        json!({"minRating":1000})
    );
}

#[test]
fn the_filters_capability_is_its_own_switch_and_older_nodes_lack_it() {
    let c: knowledge::Capabilities =
        serde_json::from_value(json!({"read":true,"puzzleSelection":true,"contract":"draft-1"}))
            .unwrap();
    assert!(c.puzzle_selection && !c.puzzle_filters);
    let c: knowledge::Capabilities = serde_json::from_value(
        json!({"read":true,"puzzleSelection":true,"puzzleFilters":true,"contract":"draft-1"}),
    )
    .unwrap();
    assert!(c.puzzle_filters);
    assert_eq!(
        serde_json::to_value(&c).unwrap()["puzzleFilters"],
        json!(true)
    );
}

#[test]
fn an_opening_and_a_daily_filter_are_written_only_when_asked_for() {
    let p: knowledge::PuzzlesParams = serde_json::from_value(
        json!({"opening":"Sicilian_Defense","daily":true,"order":"random","limit":1}),
    )
    .unwrap();
    assert_eq!(p.opening.as_deref(), Some("Sicilian_Defense"));
    assert_eq!(p.daily, Some(true));
    assert!(p.validate().is_ok());
    let plain = knowledge::PuzzlesParams {
        theme: Some("fork".into()),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        json!({"theme":"fork"})
    );
    for bad in ["", "Sicilian Defense", "Sicilian/Defense", &"a".repeat(129)] {
        let p = knowledge::PuzzlesParams {
            opening: Some(bad.to_string()),
            ..Default::default()
        };
        assert!(p.validate().is_err(), "{bad:?}");
    }
    assert!(knowledge::valid_opening("Queens_Gambit-Declined"));
}

#[test]
fn facets_list_openings_only_when_the_node_has_them() {
    let none: knowledge::FacetsResult =
        serde_json::from_value(json!({"bands":[],"total":[],"themes":[],"sources":[]})).unwrap();
    assert!(none.openings.is_empty());
    assert_eq!(
        serde_json::to_value(&none).unwrap(),
        json!({"bands":[],"total":[],"themes":[],"sources":[]})
    );
    let some: knowledge::FacetsResult = serde_json::from_value(json!({
        "bands":[{"minRating":0}],"total":[9],"themes":[],"sources":[],
        "openings":[{"opening":"Italian_Game","counts":[4]}]
    }))
    .unwrap();
    assert_eq!(some.openings[0].opening, "Italian_Game");
    assert_eq!(some.openings[0].counts, [4]);
}
