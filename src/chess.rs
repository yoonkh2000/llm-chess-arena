use std::collections::HashMap;

use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, KnownOutcome, Outcome, Position, fen::Fen,
    san::SanPlus, uci::UciMove, zobrist::Zobrist64,
};
use thiserror::Error;

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Clone, Debug, PartialEq)]
pub struct PlayedMove {
    pub uci: String,
    pub san: String,
    pub fen_before: String,
    pub fen_after: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameEnd {
    pub result: String,
    pub termination: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum ChessError {
    #[error("UCI 형식이 아닙니다. 예: e2e4, e7e8q")]
    InvalidFormat,
    #[error("현재 포지션에서 둘 수 없는 수입니다")]
    IllegalMove,
    #[error("FEN을 읽을 수 없습니다")]
    InvalidFen,
    #[error("이미 종료된 게임입니다")]
    GameOver,
}

#[derive(Clone, Debug)]
pub struct ChessGame {
    position: Chess,
    repetitions: HashMap<u64, u8>,
    ply: u32,
}

impl Default for ChessGame {
    fn default() -> Self {
        let position = Chess::default();
        let mut repetitions = HashMap::new();
        repetitions.insert(position_hash(&position), 1);
        Self {
            position,
            repetitions,
            ply: 0,
        }
    }
}

impl ChessGame {
    pub fn from_fen(fen: &str) -> Result<Self, ChessError> {
        let parsed: Fen = fen.parse().map_err(|_| ChessError::InvalidFen)?;
        let position: Chess = parsed
            .into_position(CastlingMode::Standard)
            .map_err(|_| ChessError::InvalidFen)?;
        let mut repetitions = HashMap::new();
        repetitions.insert(position_hash(&position), 1);
        Ok(Self {
            position,
            repetitions,
            ply: 0,
        })
    }

    pub fn replay(initial_fen: &str, moves: &[String]) -> Result<Self, ChessError> {
        let mut game = Self::from_fen(initial_fen)?;
        for uci in moves {
            game.play_uci(uci)?;
        }
        Ok(game)
    }

    pub fn fen(&self) -> String {
        Fen::from_position(&self.position, EnPassantMode::Legal).to_string()
    }

    pub fn side_to_move(&self) -> crate::model::Side {
        match self.position.turn() {
            Color::White => crate::model::Side::White,
            Color::Black => crate::model::Side::Black,
        }
    }

    pub fn ply(&self) -> u32 {
        self.ply
    }

    pub fn legal_moves(&self) -> Vec<String> {
        self.position
            .legal_moves()
            .iter()
            .map(|mv| mv.to_uci(self.position.castles().mode()).to_string())
            .collect()
    }

    pub fn play_uci(&mut self, text: &str) -> Result<PlayedMove, ChessError> {
        if self.game_end().is_some() {
            return Err(ChessError::GameOver);
        }
        let normalized = text.trim().to_ascii_lowercase();
        let uci: UciMove = normalized.parse().map_err(|_| ChessError::InvalidFormat)?;
        let mv = uci
            .to_move(&self.position)
            .map_err(|_| ChessError::IllegalMove)?;
        let fen_before = self.fen();
        let san = SanPlus::from_move(self.position.clone(), mv).to_string();
        self.position.play_unchecked(mv);
        self.ply += 1;
        *self
            .repetitions
            .entry(position_hash(&self.position))
            .or_insert(0) += 1;
        Ok(PlayedMove {
            uci: normalized,
            san,
            fen_before,
            fen_after: self.fen(),
        })
    }

    pub fn game_end(&self) -> Option<GameEnd> {
        match self.position.outcome() {
            Outcome::Known(KnownOutcome::Decisive {
                winner: Color::White,
            }) => {
                return Some(GameEnd {
                    result: "1-0".into(),
                    termination: "체크메이트".into(),
                });
            }
            Outcome::Known(KnownOutcome::Decisive {
                winner: Color::Black,
            }) => {
                return Some(GameEnd {
                    result: "0-1".into(),
                    termination: "체크메이트".into(),
                });
            }
            Outcome::Known(KnownOutcome::Draw) => {
                let termination = if self.position.is_stalemate() {
                    "스테일메이트"
                } else {
                    "기물 부족"
                };
                return Some(GameEnd {
                    result: "1/2-1/2".into(),
                    termination: termination.into(),
                });
            }
            Outcome::Unknown => {}
        }
        if self.position.halfmoves() >= 100 {
            return Some(GameEnd {
                result: "1/2-1/2".into(),
                termination: "50수 규칙".into(),
            });
        }
        if self.repetitions.values().any(|count| *count >= 3) {
            return Some(GameEnd {
                result: "1/2-1/2".into(),
                termination: "3회 반복".into(),
            });
        }
        if self.ply >= 200 {
            return Some(GameEnd {
                result: "1/2-1/2".into(),
                termination: "200 ply 제한".into(),
            });
        }
        None
    }
}

fn position_hash(position: &Chess) -> u64 {
    position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

pub fn pieces_from_fen(fen: &str) -> [Option<char>; 64] {
    let mut squares = [None; 64];
    let board = fen.split_whitespace().next().unwrap_or("");
    let mut index = 0usize;
    for ch in board.chars() {
        match ch {
            '/' => {}
            '1'..='8' => index = index.saturating_add(ch.to_digit(10).unwrap_or(0) as usize),
            piece if index < 64 => {
                squares[index] = Some(piece);
                index += 1;
            }
            _ => {}
        }
    }
    squares
}

pub fn unicode_piece(piece: char) -> &'static str {
    match piece {
        'K' => "♔",
        'Q' => "♕",
        'R' => "♖",
        'B' => "♗",
        'N' => "♘",
        'P' => "♙",
        'k' => "♚",
        'q' => "♛",
        'r' => "♜",
        'b' => "♝",
        'n' => "♞",
        'p' => "♟",
        _ => "",
    }
}

pub fn square_name(index: usize) -> String {
    let file = (b'a' + (index % 8) as u8) as char;
    let rank = 8 - index / 8;
    format!("{file}{rank}")
}
