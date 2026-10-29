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
        source: None,
    }
}

#[test]
fn the_capability_names_its_contract_and_older_nodes_have_none() {
    let value = json!({"protocol":"1.0","node":{"name":"chessdb","version":"0.1","license":"LicenseRef-Proprietary"},"capabilities":{"preferences":{"read":true,"write":true},"games":{"read":true,"write":true,"search":true,"delete":true,"import":true,"source":true,"contract":"draft-1","future":1}},"limits":{"maxMessageBytes":65536}});
    let result: lifecycle::InitializeResult = serde_json::from_value(value).unwrap();
    let games = result.capabilities.games.unwrap();
    assert!(games.read && games.write && games.search && games.delete && games.import);
    assert!(games.source);
    assert_eq!(games.contract.as_deref(), Some(games::CONTRACT));
    // A node from before searching doesn't say it searches.
    let unsearched: games::Capabilities =
        serde_json::from_value(json!({"read":true,"write":true,"contract":"draft-1"})).unwrap();
    assert!(!unsearched.search && !unsearched.delete && !unsearched.import && !unsearched.source);
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
    assert!(bare.players.is_none() && bare.clock.is_none() && bare.source.is_none());
    assert_eq!(bare.validate(), Ok(()));
    assert!(!serde_json::to_string(&bare).unwrap().contains("source"));
}

#[test]
fn a_game_keeps_the_text_it_came_from() {
    let text = "[Event \"Paris\"]\n\n1. e4 e5 2. Nf3 d6!? (2... Nc6) $1 *\n";
    let value = json!({"id":"a","version":1,"white":"A","black":"B","result":"*","plies":4,"pgn":"[Event \"Paris\"]\n\n1. e4 e5 2. Nf3 d6 *\n","source":text});
    let read: games::Game = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(read.source.as_deref(), Some(text));
    assert_eq!(read.validate(), Ok(()));
    assert_eq!(serde_json::to_value(&read).unwrap(), value);
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
        query: Some("Carlsen 2019".into()),
    };
    assert_eq!(
        serde_json::to_value(&page).unwrap(),
        json!({"limit":5,"after":"cursor","query":"Carlsen 2019"})
    );
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
    let delete = games::DeleteParams {
        id: "a-game".into(),
    };
    let request = Request::new::<games::Delete>(5, &delete);
    assert_eq!(request.method, "games/delete");
    assert_eq!(request.params::<games::Delete>().unwrap(), delete);
    let deleted: games::Deleted = serde_json::from_value(json!({"id":"a-game"})).unwrap();
    assert_eq!(deleted.id, "a-game");

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
    let cases: [fn(&mut games::Game); 13] = [
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
        |g| g.source = Some(String::new()),
        |g| g.source = Some("e".repeat(games::MAX_SOURCE_BYTES + 1)),
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

    // A source as long as the PGN, beside it, is past the game's bound: a client keeps
    // the source only when the game still fits.
    let mut both = game();
    both.pgn = "e".repeat(games::MAX_PGN_BYTES);
    both.source = Some(both.pgn.clone());
    assert!(both.validate().is_err());
    both.source = Some("e".repeat(games::MAX_GAME_BYTES - games::MAX_PGN_BYTES - 1024));
    assert_eq!(both.validate(), Ok(()));
}

#[test]
fn a_query_has_words_and_keeps_within_its_bounds() {
    assert!(games::valid_query("Carlsen"));
    assert!(games::valid_query(" Carlsen  2019 "));
    assert!(games::valid_query(&"q".repeat(games::MAX_QUERY_BYTES)));
    for bad in ["", "   ", "a\nb", &"q".repeat(games::MAX_QUERY_BYTES + 1)] {
        assert!(!games::valid_query(bad), "{bad:?}");
    }
}

#[test]
fn a_query_matches_names_and_tags_but_not_moves() {
    let pgn = "[Event \"Rapid \\\"Open\\\"\"]\n[Site \"Oslo NOR\"]\n[Date \"2019.06.04\"]\n\
               [White \"Carlsen, Magnus\"]\n\n1. e4 {Najdorf next} c5 *\n";
    assert_eq!(
        games::tag_values(pgn),
        [
            "Rapid \"Open\"",
            "Oslo NOR",
            "2019.06.04",
            "Carlsen, Magnus"
        ]
    );
    let matches = |query| games::matches(query, "Carlsen, Magnus", "Player", pgn);
    // Every word, ignoring case, in a name or a tag's value.
    assert!(matches("carlsen"));
    assert!(matches("MAGNUS oslo"));
    assert!(matches("2019.06 \"open\""));
    assert!(matches("play"));
    // A word nowhere, or only in the moves, doesn't match.
    assert!(!matches("carlsen tal"));
    assert!(!matches("najdorf"));
    assert!(!matches("e4"));
    // Case is Unicode's.
    assert!(games::matches("ÉLODIE", "Élodie", "B", "*"));
    // Tags several to a line, or none, or broken.
    assert_eq!(
        games::tag_values("[Event \"A\"][Site \"B\"] 1. e4 *"),
        ["A", "B"]
    );
    assert!(games::tag_values("1. e4 e5 *").is_empty());
    assert_eq!(
        games::tag_values("[Event \"A\"]\n[Round]\n[Site \"B\"]"),
        ["A"]
    );
}

/// A game of `id` whose PGN is `pgn_bytes` long.
fn imported(id: &str, pgn_bytes: usize) -> games::Game {
    games::Game {
        id: id.into(),
        version: 1,
        white: "Morphy".into(),
        black: "Duke of Brunswick".into(),
        result: games::GameResult::WhiteWon,
        plies: 33,
        pgn: "e".repeat(pgn_bytes),
        players: None,
        clock: None,
        source: None,
    }
}

#[test]
fn an_import_reads_as_the_spec_writes_it() {
    let params: games::ImportParams = serde_json::from_value(json!({"games":[
        {"id":"a","version":1,"white":"A","black":"B","result":"1-0","plies":0,"pgn":"1-0"}
    ]}))
    .unwrap();
    assert_eq!(params.validate(), Ok(()));
    let request = Request::new::<games::Import>(1, &params);
    assert_eq!(request.method, "games/import");
    assert_eq!(request.params::<games::Import>().unwrap(), params);
    let result: games::ImportResult =
        serde_json::from_value(json!({"games":["saved","kept","refused"]})).unwrap();
    assert_eq!(
        result.games,
        [
            games::Imported::Saved,
            games::Imported::Kept,
            games::Imported::Refused
        ]
    );
}

#[test]
fn an_import_out_of_bounds_is_refused() {
    let batch = |games: Vec<games::Game>| games::ImportParams { games };
    assert!(batch(vec![]).validate().is_err(), "no games");
    let many = (0..=games::MAX_IMPORT_GAMES)
        .map(|n| imported(&format!("g{n}"), 10))
        .collect::<Vec<_>>();
    assert!(batch(many[1..].to_vec()).validate().is_ok());
    assert!(batch(many).validate().is_err(), "one game too many");
    assert!(
        batch(vec![imported("a", 10), imported("a", 20)])
            .validate()
            .is_err(),
        "the same identity twice"
    );
    let mut bad = imported("b", 10);
    bad.white.clear();
    assert!(batch(vec![imported("a", 10), bad]).validate().is_err());
    let half = games::MAX_PGN_BYTES - 100;
    assert!(
        batch(vec![imported("a", half), imported("b", half)])
            .validate()
            .is_err(),
        "two long games are more bytes than one message holds"
    );
}

#[test]
fn a_batcher_keeps_the_order_and_each_batch_within_the_bounds() {
    let mut batcher = games::Batcher::default();
    let mut batches = Vec::new();
    let mut sent = Vec::new();
    // Short games fill batches by count, long ones by bytes.
    for n in 0..250 {
        let game = imported(&format!("short-{n}"), 400);
        sent.push(game.id.clone());
        batches.extend(batcher.push(game));
    }
    for n in 0..9 {
        let game = imported(&format!("long-{n}"), games::MAX_PGN_BYTES);
        sent.push(game.id.clone());
        batches.extend(batcher.push(game));
    }
    batches.extend(batcher.finish());
    let sizes: Vec<usize> = batches.iter().map(|b| b.games.len()).collect();
    assert_eq!(&sizes[..2], [games::MAX_IMPORT_GAMES; 2]);
    // The last short games share a batch with the first long one, which fills it by bytes.
    assert_eq!(sizes[2], 51, "{sizes:?}");
    assert!(sizes[3..].iter().all(|n| *n == 1), "{sizes:?}");
    for batch in &batches {
        assert_eq!(batch.validate(), Ok(()));
        let line = serde_json::to_vec(&cnp::Message::from(Request::new::<games::Import>(
            i64::MAX,
            batch,
        )))
        .unwrap();
        assert!(
            line.len() < games::MESSAGE_BYTES as usize,
            "{} bytes",
            line.len()
        );
    }
    let order: Vec<String> = batches
        .into_iter()
        .flat_map(|b| b.games)
        .map(|g| g.id)
        .collect();
    assert_eq!(order, sent);
    assert!(games::Batcher::default().finish().is_none());
}
