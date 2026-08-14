use llm_chess_arena::{
    chess::{ChessGame, pieces_from_fen},
    model::{AppData, GameMode, Side, SidePreference},
    prompt::parse_llm_response,
    rating::{benchmark_estimate, updated_rating},
    stockfish::{parse_bestmove, parse_info},
};

#[test]
fn app_starts_with_me_and_five_modes() {
    let data = AppData::default();
    assert_eq!(data.profiles[0].name, "나");
    assert_eq!(data.profiles[0].elo, 1200.0);
    assert_eq!(GameMode::ALL.len(), 5);
}

#[test]
fn equal_ratings_move_sixteen_points() {
    assert_eq!(updated_rating(1200.0, 1200.0, 1.0), 1216.0);
}

#[test]
fn explicit_and_random_side_preferences_resolve() {
    assert_eq!(SidePreference::White.resolve(0.9), Side::White);
    assert_eq!(SidePreference::Black.resolve(0.1), Side::Black);
    assert_eq!(SidePreference::Random.resolve(0.0), Side::White);
    assert_eq!(SidePreference::Random.resolve(0.499), Side::White);
    assert_eq!(SidePreference::Random.resolve(0.5), Side::Black);
    assert_eq!(SidePreference::Random.resolve(0.999), Side::Black);
}

#[test]
fn benchmark_requires_five_games() {
    assert!(benchmark_estimate(&[(1500.0, 1.0, true); 4]).is_none());
    assert!(benchmark_estimate(&[(1500.0, 0.5, true); 5]).is_some());
}

#[test]
fn chess_core_validates_and_ends_a_game() {
    let mut game = ChessGame::default();
    assert_eq!(game.legal_moves().len(), 20);
    for mv in ["f2f3", "e7e5", "g2g4", "d8h4"] {
        game.play_uci(mv).unwrap();
    }
    let end = game.game_end().unwrap();
    assert_eq!(end.result, "0-1");
    assert_eq!(
        pieces_from_fen(&game.fen())
            .iter()
            .filter(|p| p.is_some())
            .count(),
        32
    );
}

#[test]
fn promotion_requires_an_explicit_piece_suffix() {
    let game = ChessGame::from_fen("7k/Pp6/8/8/8/8/8/7K w - - 0 1").unwrap();
    let legal = game.legal_moves();
    assert!(!legal.contains(&"a7a8".to_string()));
    for promotion in ["a7a8q", "a7a8r", "a7a8b", "a7a8n"] {
        assert!(legal.contains(&promotion.to_string()));
    }
}

#[test]
fn parses_common_llm_and_engine_responses() {
    assert_eq!(
        parse_llm_response("{\"move\":\"e2e4\"}").unwrap().uci,
        "e2e4"
    );
    assert_eq!(
        parse_llm_response("make_move(\"g1f3\")").unwrap().uci,
        "g1f3"
    );
    assert_eq!(parse_bestmove("bestmove e2e4 ponder e7e5").unwrap(), "e2e4");
    let line = parse_info("info depth 14 score cp 32 nodes 9 pv e2e4 e7e5").unwrap();
    assert_eq!(line.depth, 14);
    assert_eq!(line.score_cp, Some(32));
}
