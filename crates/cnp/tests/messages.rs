//! The envelope and the types: what goes over the wire, both ways.

use cnp::engine::{self, Bound, Position, Score, ScoreValue, SearchLimits, SearchParams};
use cnp::lifecycle::{self, Version, negotiate};
use cnp::voice;
use cnp::{Id, Message, NoParams, Notification, Request, Response, ResponseError, code};
use serde_json::{Value, json};

fn parse(value: Value) -> Message {
    Message::from_value(value).expect("a valid message")
}

#[test]
fn a_request_a_notification_and_a_response_are_told_apart() {
    assert!(matches!(
        parse(json!({"jsonrpc": "2.0", "id": 1, "method": "ping"})),
        Message::Request(Request {
            id: Id::Number(1),
            ..
        })
    ));
    assert!(matches!(
        parse(json!({"jsonrpc": "2.0", "method": "exit"})),
        Message::Notification(_)
    ));
    assert!(matches!(
        parse(json!({"jsonrpc": "2.0", "id": "a", "result": 3})),
        Message::Response(Response {
            id: Some(Id::String(_)),
            outcome: Ok(_)
        })
    ));
}

#[test]
fn a_null_result_is_a_result() {
    let message = parse(json!({"jsonrpc": "2.0", "id": 4, "result": null}));
    let Message::Response(response) = message.clone() else {
        panic!("not a response");
    };
    assert_eq!(response.outcome, Ok(Value::Null));
    assert_eq!(response.result::<lifecycle::Ping>(), Ok(()));
    // And it goes back out with its `null`.
    assert_eq!(
        message.to_value(),
        json!({"jsonrpc": "2.0", "id": 4, "result": null})
    );
}

#[test]
fn an_error_with_a_null_id_keeps_its_null() {
    let message = parse(json!({"jsonrpc": "2.0", "id": null,
        "error": {"code": -32700, "message": "not JSON"}}));
    let Message::Response(response) = &message else {
        panic!("not a response");
    };
    assert_eq!(response.id, None);
    assert_eq!(
        response.outcome.as_ref().unwrap_err().code,
        code::PARSE_ERROR
    );
    assert_eq!(message.to_value()["id"], Value::Null);
}

#[test]
fn invalid_messages_say_why_and_keep_the_id_they_could_read() {
    let cases = [
        (json!([1, 2]), None),
        (json!({"id": 1, "method": "ping"}), Some(Id::Number(1))),
        (
            json!({"jsonrpc": "1.0", "id": 2, "method": "ping"}),
            Some(Id::Number(2)),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 3, "method": 7}),
            Some(Id::Number(3)),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 4, "method": "ping", "params": [1]}),
            Some(Id::Number(4)),
        ),
        (json!({"jsonrpc": "2.0", "id": 5}), Some(Id::Number(5))),
        (
            json!({"jsonrpc": "2.0", "id": 6, "result": 1, "error": {"code": 1, "message": "x"}}),
            Some(Id::Number(6)),
        ),
        (json!({"jsonrpc": "2.0", "id": 1.5, "method": "ping"}), None),
        (
            json!({"jsonrpc": "2.0", "id": null, "method": "ping"}),
            None,
        ),
        (json!({"jsonrpc": "2.0"}), None),
    ];
    for (value, id) in cases {
        let error = Message::from_value(value.clone()).expect_err(&value.to_string());
        assert_eq!(error.id, id, "{value}");
        assert!(!error.reason.is_empty());
    }
}

#[test]
fn typed_requests_round_trip_through_json() {
    let params = SearchParams::new(Position::start().with_moves(["e2e4", "e7e5"]))
        .limits(SearchLimits::depth(12));
    let request = Request::new::<engine::Search>(7, &params);
    let text = serde_json::to_string(&Message::from(request)).unwrap();
    let Ok(Message::Request(back)) = text.parse::<Message>() else {
        panic!("not a request: {text}");
    };
    assert!(back.is::<engine::Search>());
    assert_eq!(back.params::<engine::Search>().unwrap(), params);
    // Defaults are left out: a plain search is short.
    assert_eq!(
        back.params.unwrap(),
        json!({"position": {"fen": engine::START_FEN, "moves": ["e2e4", "e7e5"]},
               "limits": {"depth": 12}})
    );
}

#[test]
fn a_listen_and_its_audio_leave_out_what_isnt_said() {
    let listen = Request::new::<voice::Listen>(30, &voice::ListenParams::new(16_000));
    assert_eq!(
        Message::from(listen).to_value(),
        json!({"jsonrpc": "2.0", "id": 30, "method": "voice/listen",
               "params": {"format": "pcmF32", "sampleRate": 16000}})
    );
    // The last piece of audio may be only the end.
    let end = voice::AudioParams {
        id: Id::Number(30),
        audio: None,
        end: true,
    };
    assert_eq!(
        Message::from(Notification::new::<voice::Audio>(&end)).to_value(),
        json!({"jsonrpc": "2.0", "method": "voice/audio", "params": {"id": 30, "end": true}})
    );
    let heard = parse(json!({"jsonrpc": "2.0", "method": "voice/heard",
        "params": {"id": 30, "text": "", "startMs": 10, "endMs": 900}}));
    let Message::Notification(heard) = heard else {
        panic!("not a notification")
    };
    let heard = heard.params::<voice::Heard>().unwrap();
    assert_eq!((heard.text.as_str(), heard.words.len()), ("", 0));
}

#[test]
fn a_method_without_params_sends_none_and_reads_none() {
    let request = Request::new::<lifecycle::Ping>(1, &NoParams);
    assert_eq!(request.params, None);
    assert_eq!(request.params::<lifecycle::Ping>(), Ok(NoParams));
    // Params sent anyway are ignored, like unknown fields.
    let with_params =
        parse(json!({"jsonrpc": "2.0", "id": 2, "method": "ping", "params": {"x": 1}}));
    let Message::Request(with_params) = with_params else {
        panic!()
    };
    assert_eq!(with_params.params::<lifecycle::Ping>(), Ok(NoParams));
    let exit = Notification::new::<lifecycle::Exit>(&NoParams);
    assert_eq!(
        Message::from(exit).to_value(),
        json!({"jsonrpc": "2.0", "method": "exit"})
    );
}

#[test]
fn params_of_the_wrong_shape_are_an_invalid_params_error() {
    let request = parse(json!({"jsonrpc": "2.0", "id": 1, "method": "engine/search",
        "params": {"position": {"moves": []}}}));
    let Message::Request(request) = request else {
        panic!()
    };
    let error = request.params::<engine::Search>().unwrap_err();
    assert_eq!(error.code, code::INVALID_PARAMS);
    assert!(error.message.contains("fen"), "{}", error.message);
}

#[test]
fn unknown_fields_are_ignored() {
    let params: SearchParams = serde_json::from_value(json!({
        "position": {"fen": engine::START_FEN},
        "limits": {"depth": 3, "futureLimit": 1},
        "futureOption": {"anything": true}
    }))
    .unwrap();
    assert_eq!(params.limits.depth, Some(3));
}

#[test]
fn scores_are_centipawns_or_mates_with_their_bound() {
    assert_eq!(
        serde_json::to_value(Score::cp(18)).unwrap(),
        json!({"cp": 18})
    );
    assert_eq!(
        serde_json::to_value(Score::mate(-3)).unwrap(),
        json!({"mate": -3})
    );
    let bound: Score = serde_json::from_value(json!({"cp": -40, "bound": "upper"})).unwrap();
    assert_eq!(bound.value, ScoreValue::Centipawns(-40));
    assert_eq!(bound.bound, Bound::Upper);
    assert_eq!(
        serde_json::to_value(bound).unwrap(),
        json!({"cp": -40, "bound": "upper"})
    );
    for bad in [
        json!({}),
        json!({"cp": 1, "mate": 2}),
        json!({"cp": 1, "bound": "both"}),
    ] {
        assert!(
            serde_json::from_value::<Score>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_search_result_with_no_legal_move_has_a_null_best_move() {
    let result: engine::SearchResult = serde_json::from_value(json!({
        "bestMove": null, "pv": [], "depth": 0, "nodes": 1, "timeMs": 0
    }))
    .unwrap();
    assert_eq!(result.best_move, None);
    assert_eq!(
        serde_json::to_value(&result).unwrap()["bestMove"],
        Value::Null
    );
}

#[test]
fn a_ponder_search_says_so_and_its_hit_names_it() {
    let mut params = SearchParams::new(Position::start().with_moves(["e2e4", "e7e5"]))
        .limits(SearchLimits::depth(12));
    params.ponder = true;
    let value = serde_json::to_value(&params).unwrap();
    assert_eq!(value["ponder"], json!(true));
    assert_eq!(
        serde_json::from_value::<SearchParams>(value).unwrap(),
        params
    );
    // A search that isn't pondering leaves the flag out.
    params.ponder = false;
    assert_eq!(serde_json::to_value(&params).unwrap().get("ponder"), None);

    let hit = Request::new::<engine::PonderHit>(8, &engine::PonderHitParams { id: Id::Number(7) });
    let Message::Request(hit) = parse(Message::from(hit).to_value()) else {
        panic!("not a request");
    };
    assert_eq!(hit.method, "engine/ponderHit");
    assert_eq!(hit.params.unwrap(), json!({"id": 7}));

    // Only a ponder search's result says how long it pondered.
    let result: engine::SearchResult = serde_json::from_value(json!({
        "bestMove": "g1f3", "pv": ["g1f3"], "depth": 9, "nodes": 900, "timeMs": 700,
        "ponderMs": 500
    }))
    .unwrap();
    assert_eq!(result.ponder_ms, Some(500));
    let plain: engine::SearchResult = serde_json::from_value(json!({
        "bestMove": "g1f3", "pv": ["g1f3"], "depth": 9, "nodes": 900, "timeMs": 700
    }))
    .unwrap();
    assert_eq!(plain.ponder_ms, None);
    assert_eq!(serde_json::to_value(&plain).unwrap().get("ponderMs"), None);
    // The capability is off unless offered.
    let offered: engine::EngineCapabilities = serde_json::from_value(json!({})).unwrap();
    assert!(!offered.ponder);
}

#[test]
fn an_evaluation_is_a_score_and_its_terms_for_each_side() {
    let result = engine::EvaluateResult {
        score: Score::cp(-1),
        terms: vec![
            engine::Term {
                id: "material".to_string(),
                name: "Material".to_string(),
                description: None,
                white: 3940,
                black: 3940,
            },
            engine::Term {
                id: "tempo".to_string(),
                name: "Tempo".to_string(),
                description: Some("A bonus for the side to move.".to_string()),
                white: 10,
                black: 0,
            },
        ],
        quiet: None,
        search_score: None,
        pieces: None,
    };
    let value = serde_json::to_value(&result).unwrap();
    // A term without a description leaves it out.
    assert_eq!(
        value,
        json!({"score": {"cp": -1}, "terms": [
            {"id": "material", "name": "Material", "white": 3940, "black": 3940},
            {"id": "tempo", "name": "Tempo", "description": "A bonus for the side to move.",
             "white": 10, "black": 0}
        ]})
    );
    assert_eq!(
        serde_json::from_value::<engine::EvaluateResult>(value).unwrap(),
        result
    );
    // The capability is off unless offered.
    let offered: engine::EngineCapabilities = serde_json::from_value(json!({})).unwrap();
    assert!(!offered.evaluate);
}

#[test]
fn an_evaluation_can_settle_captures_and_value_each_piece() {
    let position = engine::Position::start().with_moves(["e2e4", "d7d5", "e4d5"]);
    // Nothing more asked: the flags stay out, as for a node that doesn't know them.
    let plain = engine::EvaluateParams::new(position.clone());
    let value = serde_json::to_value(&plain).unwrap();
    assert_eq!(value.get("quiet"), None);
    assert_eq!(value.get("pieces"), None);
    let asked = engine::EvaluateParams {
        quiet: true,
        pieces: true,
        ..plain
    };
    let value = serde_json::to_value(&asked).unwrap();
    assert_eq!(value["quiet"], json!(true));
    assert_eq!(value["pieces"], json!(true));
    assert_eq!(
        serde_json::from_value::<engine::EvaluateParams>(value).unwrap(),
        asked
    );

    let result = engine::EvaluateResult {
        score: Score::cp(-17),
        terms: vec![],
        quiet: Some(vec!["d8d5".to_string()]),
        search_score: Some(Score::cp(34)),
        pieces: Some(vec![engine::PieceValue {
            square: "d5".to_string(),
            value: 912,
        }]),
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        value,
        json!({"score": {"cp": -17}, "terms": [], "quiet": ["d8d5"],
               "searchScore": {"cp": 34}, "pieces": [{"square": "d5", "value": 912}]})
    );
    assert_eq!(
        serde_json::from_value::<engine::EvaluateResult>(value).unwrap(),
        result
    );
    // Asked to settle an already quiet position: no moves, which isn't the same as not asked.
    let settled: engine::EvaluateResult =
        serde_json::from_value(json!({"score": {"cp": 5}, "terms": [], "quiet": []})).unwrap();
    assert_eq!(settled.quiet, Some(vec![]));
    let unasked: engine::EvaluateResult =
        serde_json::from_value(json!({"score": {"cp": 5}, "terms": []})).unwrap();
    assert_eq!(
        (unasked.quiet, unasked.search_score, unasked.pieces),
        (None, None, None)
    );
    // Checkmated where the captures stop: mate in 0, and no piece values.
    let mated: engine::EvaluateResult =
        serde_json::from_value(json!({"score": {"mate": 0}, "terms": [], "quiet": []})).unwrap();
    assert_eq!(mated.score, Score::mate(0));
    // The capabilities are off unless offered.
    let offered: engine::EngineCapabilities = serde_json::from_value(json!({})).unwrap();
    assert!(!offered.quiet && !offered.pieces);
    let offered: engine::EngineCapabilities =
        serde_json::from_value(json!({"quiet": true, "pieces": true})).unwrap();
    assert!(offered.quiet && offered.pieces);
}

#[test]
fn a_tree_request_leaves_out_what_it_doesn_t_ask_and_defaults_its_plies() {
    let plain = engine::TreeParams::new(Position::start(), 12);
    let value = serde_json::to_value(&plain).unwrap();
    assert_eq!(
        value,
        json!({"position": {"fen": engine::START_FEN}, "depth": 12})
    );
    assert_eq!(plain.recorded_plies(), engine::DEFAULT_TREE_PLIES);
    let asked = engine::TreeParams {
        plies: Some(3),
        limit: Some(5),
        ..plain
    };
    assert_eq!(asked.recorded_plies(), 3);
    let value = serde_json::to_value(&asked).unwrap();
    assert_eq!(
        (value["plies"].clone(), value["limit"].clone()),
        (json!(3), json!(5))
    );
    assert_eq!(
        serde_json::from_value::<engine::TreeParams>(value).unwrap(),
        asked
    );
    // A page: the path from the root, and an offset and limit only when asked.
    let page = engine::TreeNodeParams::new("t1", ["e2e4", "c7c5"]);
    assert_eq!(
        serde_json::to_value(&page).unwrap(),
        json!({"tree": "t1", "path": ["e2e4", "c7c5"]})
    );
    let root: engine::TreeNodeParams = serde_json::from_value(json!({"tree": "t1"})).unwrap();
    assert_eq!((root.path, root.offset, root.limit), (vec![], None, None));
}

#[test]
fn a_tree_node_has_its_window_its_bounded_score_and_its_decisions() {
    use engine::{Decision, DecisionKind, ResearchReason};

    let node = engine::TreeNode {
        mv: Some("f1c4".to_string()),
        depth: 15,
        alpha: Some(Score::cp(-31)),
        beta: Some(Score::cp(-30)),
        score: Some(Score {
            value: ScoreValue::Centipawns(-12),
            bound: Bound::Lower,
        }),
        child_count: 1,
        subtree: 14,
        decisions: vec![
            Decision::new(DecisionKind::Reduced { plies: 1 }).by("lmr"),
            Decision::new(DecisionKind::Research {
                reason: ResearchReason::Depth,
            })
            .by("lmr"),
            Decision::new(DecisionKind::Cutoff {
                mv: "g8f6".to_string(),
                order: 1,
            })
            .by("alphabeta"),
        ],
    };
    let value = serde_json::to_value(&node).unwrap();
    assert_eq!(
        value,
        json!({"move": "f1c4", "depth": 15, "alpha": {"cp": -31}, "beta": {"cp": -30},
               "score": {"cp": -12, "bound": "lower"}, "childCount": 1, "subtree": 14,
               "decisions": [
                   {"kind": "reduced", "plies": 1, "technique": "lmr"},
                   {"kind": "research", "reason": "depth", "technique": "lmr"},
                   {"kind": "cutoff", "move": "g8f6", "order": 1, "technique": "alphabeta"}]})
    );
    assert_eq!(
        serde_json::from_value::<engine::TreeNode>(value).unwrap(),
        node
    );

    // The root: no move and no window. A window's missing edge means it had none.
    let root: engine::TreeNode = serde_json::from_value(
        json!({"depth": 16, "score": {"cp": 31}, "childCount": 27, "subtree": 802}),
    )
    .unwrap();
    assert_eq!((root.mv, root.alpha, root.beta), (None, None, None));
    assert!(root.decisions.is_empty());

    // Each kind, as JSON has it; a node-level one with and without a move.
    let kinds = [
        (
            json!({"kind": "table", "technique": "tt"}),
            DecisionKind::Table,
        ),
        (json!({"kind": "tablebase"}), DecisionKind::Tablebase),
        (
            json!({"kind": "nullMove", "technique": "nullmove"}),
            DecisionKind::NullMove,
        ),
        (
            json!({"kind": "nodePruned", "technique": "rfp"}),
            DecisionKind::NodePruned { mv: None },
        ),
        (
            json!({"kind": "nodePruned", "move": "c3d5", "technique": "probcut"}),
            DecisionKind::NodePruned {
                mv: Some("c3d5".to_string()),
            },
        ),
        (
            json!({"kind": "movePruned", "move": "a2a3", "technique": "lmp"}),
            DecisionKind::MovePruned {
                mv: "a2a3".to_string(),
            },
        ),
        (
            json!({"kind": "extended", "plies": 1, "technique": "singular"}),
            DecisionKind::Extended { plies: 1 },
        ),
        (
            json!({"kind": "research", "reason": "window", "technique": "pvs"}),
            DecisionKind::Research {
                reason: ResearchReason::Window,
            },
        ),
    ];
    for (value, kind) in kinds {
        let decision: Decision = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decision.kind, kind);
        assert_eq!(serde_json::to_value(&decision).unwrap(), value);
    }
    // A later version's kind, or reason, is read rather than refused, so a client can skip it.
    let later: Decision =
        serde_json::from_value(json!({"kind": "aspiration", "technique": "x", "by": 3})).unwrap();
    assert_eq!(later.kind, DecisionKind::Unknown);
    let later: Decision =
        serde_json::from_value(json!({"kind": "research", "reason": "aspiration"})).unwrap();
    assert_eq!(
        later.kind,
        DecisionKind::Research {
            reason: ResearchReason::Unknown
        }
    );
    // A cutoff names its move.
    assert!(serde_json::from_value::<Decision>(json!({"kind": "cutoff", "order": 1})).is_err());
}

#[test]
fn a_tree_says_what_it_holds_and_the_tree_capability_and_limits_are_offered() {
    let result: engine::TreeResult = serde_json::from_value(json!({
        "tree": "t1", "expiresMs": 300000, "depth": 4, "nodes": 900, "timeMs": 3,
        "bestMove": "e2e4", "score": {"cp": 30}, "pv": ["e2e4", "e7e5"],
        "recorded": {"plies": 2, "nodes": 61, "decisions": 75},
        "root": {"depth": 4, "score": {"cp": 30}, "childCount": 20, "subtree": 60},
        "children": []
    }))
    .unwrap();
    assert!(!result.recorded.truncated);
    assert_eq!(result.sel_depth, 0);
    // No legal move: no best move and no score, as for a search.
    let mated: engine::TreeResult = serde_json::from_value(json!({
        "tree": "t2", "expiresMs": 300000, "depth": 0, "nodes": 0, "timeMs": 0,
        "bestMove": null, "pv": [],
        "recorded": {"plies": 2, "nodes": 1, "decisions": 0},
        "root": {"depth": 0, "childCount": 0, "subtree": 0}
    }))
    .unwrap();
    assert_eq!((mated.best_move, mated.score), (None, None));
    assert!(mated.children.is_empty());

    let offered: engine::EngineCapabilities = serde_json::from_value(json!({})).unwrap();
    assert!(!offered.tree);
    let offered: engine::EngineCapabilities =
        serde_json::from_value(json!({"tree": true})).unwrap();
    assert!(offered.tree);
    // A tree's depth falls back on maxDepth, and one tree at a time is the default.
    let limits: lifecycle::Limits = serde_json::from_value(json!({"maxDepth": 64})).unwrap();
    assert_eq!(
        (limits.tree_depth(), limits.concurrent_trees()),
        (Some(64), 1)
    );
    let limits: lifecycle::Limits = serde_json::from_value(
        json!({"maxDepth": 64, "maxTreeDepth": 24, "maxTreePlies": 6, "maxConcurrentTrees": 2}),
    )
    .unwrap();
    assert_eq!(
        (
            limits.tree_depth(),
            limits.max_tree_plies,
            limits.concurrent_trees()
        ),
        (Some(24), Some(6), 2)
    );
    assert_eq!(code::EXPIRED, -32018);
}

#[test]
fn a_search_can_ask_to_follow_its_root_and_a_node_can_offer_it() {
    let plain = SearchParams::new(Position::start());
    assert_eq!(serde_json::to_value(&plain).unwrap().get("rootMs"), None);
    let following = plain.root_ms(250);
    assert_eq!(following.root_ms, Some(250));
    let value = serde_json::to_value(&following).unwrap();
    assert_eq!(value["rootMs"], json!(250));
    assert_eq!(
        serde_json::from_value::<SearchParams>(value).unwrap(),
        following
    );

    let offered: engine::EngineCapabilities = serde_json::from_value(json!({})).unwrap();
    assert!(!offered.root);
    let offered: engine::EngineCapabilities =
        serde_json::from_value(json!({"root": true})).unwrap();
    assert!(offered.root);
}

#[test]
fn following_the_root_says_the_move_its_place_the_best_and_the_effort() {
    use engine::{RootMove, RootParams, RootResearch};

    let params = RootParams {
        id: Id::Number(20),
        depth: 14,
        multipv: Some(2),
        mv: "d2d4".to_string(),
        number: 3,
        of: 26,
        best: Some("f1c4".to_string()),
        moves: vec![
            RootMove {
                mv: "f1c4".to_string(),
                nodes: 81_234,
            },
            RootMove {
                mv: "d2d4".to_string(),
                nodes: 1_203,
            },
        ],
        nodes: 190_011,
        time_ms: 250,
        research: Some(RootResearch::Better {
            mv: "d2d4".to_string(),
        }),
    };
    let notification = Notification::new::<engine::Root>(&params);
    assert_eq!(notification.method, "engine/root");
    let value = Message::from(notification).to_value();
    assert_eq!(
        value,
        json!({"jsonrpc": "2.0", "method": "engine/root", "params": {
            "id": 20, "depth": 14, "multipv": 2, "move": "d2d4", "number": 3, "of": 26,
            "best": "f1c4",
            "moves": [{"move": "f1c4", "nodes": 81234}, {"move": "d2d4", "nodes": 1203}],
            "nodes": 190011, "timeMs": 250, "research": {"kind": "better", "move": "d2d4"}}})
    );
    let Message::Notification(read) = parse(value) else {
        panic!("not a notification");
    };
    assert!(read.is::<engine::Root>());
    assert_eq!(read.params::<engine::Root>().unwrap(), params);

    // The first: one line, no best yet, nothing gone back over.
    let first: RootParams = serde_json::from_value(json!({
        "id": 3, "depth": 1, "move": "e2e4", "number": 1, "of": 20,
        "moves": [{"move": "e2e4", "nodes": 0}], "nodes": 1, "timeMs": 0
    }))
    .unwrap();
    let value = serde_json::to_value(&first).unwrap();
    assert_eq!(
        (first.multipv, first.best, first.research),
        (None, None, None)
    );
    for left_out in ["multipv", "best", "research"] {
        assert_eq!(value.get(left_out), None, "{left_out}");
    }

    // A depth's score outside its window comes with its bound.
    let kinds = [
        (
            json!({"kind": "failHigh", "score": {"cp": 45, "bound": "lower"}}),
            RootResearch::FailHigh {
                score: Score {
                    value: ScoreValue::Centipawns(45),
                    bound: Bound::Lower,
                },
            },
        ),
        (
            json!({"kind": "failLow", "score": {"cp": -8, "bound": "upper"}}),
            RootResearch::FailLow {
                score: Score {
                    value: ScoreValue::Centipawns(-8),
                    bound: Bound::Upper,
                },
            },
        ),
    ];
    for (value, research) in kinds {
        assert_eq!(
            serde_json::from_value::<RootResearch>(value.clone()).unwrap(),
            research
        );
        assert_eq!(serde_json::to_value(&research).unwrap(), value);
    }
    // A later version's kind is read rather than refused, so a client can skip it.
    let later: RootResearch =
        serde_json::from_value(json!({"kind": "singular", "move": "e2e4"})).unwrap();
    assert_eq!(later, RootResearch::Unknown);
    // A better move names itself, and a window's failure its score.
    assert!(serde_json::from_value::<RootResearch>(json!({"kind": "better"})).is_err());
    assert!(serde_json::from_value::<RootResearch>(json!({"kind": "failHigh"})).is_err());
}

#[test]
fn versions_are_text_and_agree_on_the_major_version() {
    let v: Version = serde_json::from_value(json!("1.3")).unwrap();
    assert_eq!(v, Version { major: 1, minor: 3 });
    assert_eq!(serde_json::to_value(v).unwrap(), json!("1.3"));
    assert!(serde_json::from_value::<Version>(json!("1")).is_err());
    assert!(serde_json::from_value::<Version>(json!(1.0)).is_err());
    let node = Version { major: 1, minor: 2 };
    assert_eq!(negotiate(Version { major: 1, minor: 5 }, node), Some(node));
    assert_eq!(
        negotiate(Version { major: 1, minor: 0 }, node),
        Some(Version { major: 1, minor: 0 })
    );
    assert_eq!(negotiate(Version { major: 2, minor: 0 }, node), None);
}

#[test]
fn errors_carry_data() {
    let data = lifecycle::IncompatibleVersionData {
        supported: vec![lifecycle::PROTOCOL_VERSION],
    };
    let error = ResponseError::new(code::INCOMPATIBLE_VERSION, "speaks 1.x").with_data(&data);
    let response = Response::error(Some(Id::Number(1)), error);
    assert_eq!(
        Message::from(response).to_value(),
        json!({"jsonrpc": "2.0", "id": 1, "error": {
            "code": -32010, "message": "speaks 1.x", "data": {"supported": ["1.0"]}}})
    );
}

#[test]
fn move_text_is_two_squares_and_maybe_a_promotion() {
    for good in ["e2e4", "a7a8q", "h1h8", "e7e8n"] {
        assert!(engine::is_move_text(good), "{good}");
    }
    for bad in [
        "e2e9", "i2e4", "e7e8Q", "e7e8k", "e2", "0000", "e2-e4", "O-O", "",
    ] {
        assert!(!engine::is_move_text(bad), "{bad}");
    }
}
