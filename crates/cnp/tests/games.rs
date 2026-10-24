//! Draft games wire compatibility, typed round trips and bounds.
use cnp::{Request, Response, games, lifecycle};
use serde_json::json;

fn game() -> games::Game {
    games::Game {
        id: "5f0c2a9e-game".into(),
        version: 3,
        white: "Player".into(),
        black: "cb2026 0.1.0".into(),
        result: games::GameResult::Unfinished,
        plies: 2,
        pgn: "[Event \"Solrac game\"]\n\n1. e4 e5 *\n".into(),
        players: Some(games::Players {
            white: games::Player::Human,
            black: games::Player::Engine,
        }),
        clock: Some(games::Clock {
            control: "5+3".into(),
            times_ms: vec![[300_000, 300_000], [302_000, 300_000], [302_000, 298_500]],
        }),
    }
}

#[test]
fn the_capability_names_its_contract_and_older_nodes_have_none() {
    let value = json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"LicenseRef-Proprietary"},"capabilities":{"preferences":{"read":true,"write":true},"games":{"read":true,"write":true,"contract":"draft-1","future":1}},"limits":{"maxMessageBytes":65536}});
    let result: lifecycle::InitializeResult = serde_json::from_value(value).unwrap();
    let games = result.capabilities.games.unwrap();
    assert!(games.read && games.write);
    assert_eq!(games.contract.as_deref(), Some(games::CONTRACT));
    let older: lifecycle::InitializeResult = serde_json::from_value(
        json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"x"},"capabilities":{"preferences":{"read":true}}}),
    )
    .unwrap();
    assert!(older.capabilities.games.is_none());
}

#[test]
fn a_game_reads_as_the_spec_writes_it() {
    let value = json!({"id":"5f0c2a9e-game","version":3,"white":"Player","black":"cb2026 0.1.0","result":"*","plies":2,"pgn":"[Event \"Solrac game\"]\n\n1. e4 e5 *\n","players":{"white":"human","black":"engine"},"clock":{"control":"5+3","timesMs":[[300000,300000],[302000,300000],[302000,298500]]},"future":true});
    let read: games::Game = serde_json::from_value(value).unwrap();
    assert_eq!(read, game());
    for (result, text) in [
        (games::GameResult::WhiteWon, "1-0"),
        (games::GameResult::BlackWon, "0-1"),
        (games::GameResult::Drawn, "1/2-1/2"),
        (games::GameResult::Unfinished, "*"),
    ] {
        assert_eq!(serde_json::to_value(result).unwrap(), json!(text));
    }
    // The optional parts may be missing.
    let bare: games::Game = serde_json::from_value(
        json!({"id":"a","version":1,"white":"A","black":"B","result":"1-0","plies":0,"pgn":"1-0"}),
    )
    .unwrap();
    assert!(bare.players.is_none() && bare.clock.is_none());
    assert_eq!(bare.validate(), Ok(()));
}

#[test]
fn requests_round_trip_through_their_methods() {
    let save = games::SaveParams { game: game() };
    let request = Request::new::<games::Save>(1, &save);
    assert_eq!(request.method, "games/save");
    assert_eq!(request.params::<games::Save>().unwrap(), save);

    let list: games::ListParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(list, games::ListParams::default());
    // Without params, the newest page.
    let bare: cnp::Message =
        serde_json::from_str(r#"{"jsonrpc":"2.0","id":9,"method":"games/list"}"#).unwrap();
    let cnp::Message::Request(bare) = bare else {
        panic!("a request");
    };
    assert_eq!(
        bare.params::<games::List>().unwrap(),
        games::ListParams::default()
    );
    let request = Request::new::<games::List>(2, &list);
    assert_eq!(request.method, "games/list");
    let page = games::ListParams {
        limit: Some(5),
        after: Some("cursor".into()),
    };
    assert_eq!(
        Request::new::<games::List>(3, &page)
            .params::<games::List>()
            .unwrap(),
        page
    );

    let get = games::GetParams {
        id: "a-game".into(),
    };
    assert_eq!(
        Request::new::<games::Get>(4, &get).method,
        "games/get".to_string()
    );

    let summary = json!({"id":"a","version":2,"white":"A","black":"B","result":"0-1","plies":40,"createdAt":"2026-10-24T14:05:12.123Z","updatedAt":"2026-10-24T14:31:40.002Z"});
    let listed: games::ListResult =
        serde_json::from_value(json!({"games":[summary.clone()]})).unwrap();
    assert!(listed.next.is_none());
    assert_eq!(listed.games[0].result, games::GameResult::BlackWon);
    let saved: games::Summary = serde_json::from_value(summary).unwrap();
    assert_eq!(saved.updated_at, "2026-10-24T14:31:40.002Z");
}

#[test]
fn a_game_out_of_bounds_is_refused() {
    assert_eq!(game().validate(), Ok(()));
    let cases: [fn(&mut games::Game); 11] = [
        |g| g.id = String::new(),
        |g| g.id = "Upper".into(),
        |g| g.id = "x".repeat(games::MAX_ID_BYTES + 1),
        |g| g.version = 0,
        |g| g.version = games::MAX_VERSION + 1,
        |g| g.white = String::new(),
        |g| g.black = "a\nb".into(),
        |g| g.plies = games::MAX_PLIES + 1,
        |g| g.pgn = String::new(),
        |g| g.clock.as_mut().unwrap().times_ms.pop().map(drop).unwrap(),
        |g| g.clock.as_mut().unwrap().times_ms[0][1] = games::MAX_CLOCK_MS + 1,
    ];
    for (n, case) in cases.into_iter().enumerate() {
        let mut bad = game();
        case(&mut bad);
        assert!(bad.validate().is_err(), "case {n}: {bad:?}");
    }
}

#[test]
fn the_largest_game_fits_one_message() {
    // As long a PGN, as many moves and as long clock times as the bounds allow.
    let plies = games::MAX_PLIES;
    let mut big = game();
    big.plies = plies;
    big.pgn = "e".repeat(games::MAX_PGN_BYTES);
    big.white = "w".repeat(games::MAX_NAME_BYTES);
    big.black = "b".repeat(games::MAX_NAME_BYTES);
    big.clock = Some(games::Clock {
        control: "9".repeat(games::MAX_CONTROL_BYTES),
        times_ms: vec![[games::MAX_CLOCK_MS; 2]; plies as usize + 1],
    });
    assert_eq!(big.validate(), Ok(()));
    let stored = games::Stored {
        game: big,
        created_at: "2026-10-24T14:05:12.123Z".into(),
        updated_at: "2026-10-24T14:31:40.002Z".into(),
    };
    let answer = Response::ok::<games::Get>(cnp::Id::Number(i64::MAX), &stored);
    let line = serde_json::to_vec(&cnp::Message::from(answer)).unwrap();
    assert!(
        line.len() < games::MESSAGE_BYTES as usize,
        "{} bytes",
        line.len()
    );

    // A PGN whose escaping makes it longer than the bound is refused, not cut.
    let mut escaped = game();
    escaped.pgn = "\"".repeat(games::MAX_PGN_BYTES);
    escaped.plies = plies;
    escaped.clock = stored.game.clock.clone();
    assert!(escaped.validate().is_err());
}
