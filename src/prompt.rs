use serde_json::Value;

use crate::model::{GameRecord, PromptProtocol, Side};

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedResponse {
    pub uci: String,
    pub classification: String,
}

pub fn parse_llm_response(input: &str) -> Result<ParsedResponse, &'static str> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("응답이 비어 있습니다");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        for key in ["move", "uci", "bestmove"] {
            if let Some(uci) = value.get(key).and_then(Value::as_str).and_then(find_uci) {
                return Ok(ParsedResponse {
                    uci,
                    classification: "json".into(),
                });
            }
        }
        if let Some(arguments) = value.get("arguments") {
            for key in ["move", "uci"] {
                if let Some(uci) = arguments
                    .get(key)
                    .and_then(Value::as_str)
                    .and_then(find_uci)
                {
                    return Ok(ParsedResponse {
                        uci,
                        classification: "tool_call".into(),
                    });
                }
            }
        }
    }

    if let Some(start) = trimmed.find("make_move")
        && let Some(uci) = find_uci(&trimmed[start..])
    {
        return Ok(ParsedResponse {
            uci,
            classification: "tool_call".into(),
        });
    }

    find_uci(trimmed)
        .map(|uci| ParsedResponse {
            uci,
            classification: "plain_uci".into(),
        })
        .ok_or("UCI 수를 찾지 못했습니다")
}

fn find_uci(text: &str) -> Option<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric()))
        .map(str::to_ascii_lowercase)
        .find(|token| is_uci(token))
}

fn is_uci(token: &str) -> bool {
    let bytes = token.as_bytes();
    (bytes.len() == 4 || bytes.len() == 5)
        && matches!(bytes[0], b'a'..=b'h')
        && matches!(bytes[1], b'1'..=b'8')
        && matches!(bytes[2], b'a'..=b'h')
        && matches!(bytes[3], b'1'..=b'8')
        && (bytes.len() == 4 || matches!(bytes[4], b'q' | b'r' | b'b' | b'n'))
}

pub fn move_prompt(record: &GameRecord, side: Side, legal_moves: &[String]) -> String {
    let role = match side {
        Side::White => "White",
        Side::Black => "Black",
    };
    let history = record
        .moves
        .iter()
        .map(|mv| mv.uci.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    match record.protocol {
        PromptProtocol::ArenaDirect => format!(
            "You are playing chess as {role}.\nCurrent FEN: {}\nMoves so far (UCI): {}\nLegal moves: {}\nChoose exactly one legal move. Reply with only its UCI notation, for example e2e4.",
            record.current_fen,
            if history.is_empty() {
                "(none)"
            } else {
                &history
            },
            legal_moves.join(" ")
        ),
        PromptProtocol::PaperBenchmark => format!(
            "You are a chess-playing language model being evaluated through a UCI-style interface.\nYou play {role}.\nPosition FEN: {}\nMove history (UCI): {}\nAvailable legal actions: {}\nReturn one tool call exactly as: make_move(\"e2e4\"). You have at most 3 invalid attempts for this turn.",
            record.current_fen,
            if history.is_empty() {
                "(none)"
            } else {
                &history
            },
            legal_moves.join(" ")
        ),
    }
}

pub fn coaching_prompt(record: &GameRecord, include_engine: bool) -> String {
    let moves = record
        .moves
        .iter()
        .map(|mv| mv.san.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let engine = if include_engine && !record.review.is_empty() {
        let rows = record
            .review
            .iter()
            .map(|row| {
                format!(
                    "ply {}: best {}, quality {}",
                    row.ply, row.best_move, row.quality
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("\nOptional Stockfish review:\n{rows}")
    } else {
        String::new()
    };
    format!(
        "You are a kind but precise chess coach. Review this game in Korean.\nMatch: {} vs {}\nResult: {} ({})\nMoves (SAN): {}{}\nExplain: 1) the turning point, 2) two good decisions, 3) two improvements with concrete variations, and 4) one practice task. Do not invent engine scores that are not provided.",
        record.white.name,
        record.black.name,
        record.result.as_deref().unwrap_or("unfinished"),
        record.termination.as_deref().unwrap_or("in progress"),
        if moves.is_empty() { "(none)" } else { &moves },
        engine
    )
}

pub fn pgn(record: &GameRecord) -> String {
    let result = record.result.as_deref().unwrap_or("*");
    let mut movetext = String::new();
    for (index, mv) in record.moves.iter().enumerate() {
        if index % 2 == 0 {
            movetext.push_str(&format!("{}. ", index / 2 + 1));
        }
        movetext.push_str(&mv.san);
        movetext.push(' ');
    }
    format!(
        "[Event \"LLM Chess Arena\"]\n[Site \"Local WASM\"]\n[White \"{}\"]\n[Black \"{}\"]\n[Result \"{}\"]\n\n{}{}",
        record.white.name, record.black.name, result, movetext, result
    )
}
