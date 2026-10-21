//! The envelope and the types: what goes over the wire, both ways.

use cnp::engine::{self, Bound, Position, Score, ScoreValue, SearchLimits, SearchParams};
use cnp::lifecycle::{self, Version, negotiate};
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
