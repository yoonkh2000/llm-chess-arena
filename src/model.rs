use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
pub const STARTING_ELO: f64 = 1200.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerKind {
    Human,
    Llm,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub id: Uuid,
    pub kind: PlayerKind,
    pub name: String,
    pub model: String,
    pub elo: f64,
    pub active: bool,
}

impl PlayerProfile {
    pub fn new(kind: PlayerKind, name: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            name: name.into(),
            model: model.into(),
            elo: STARTING_ELO,
            active: true,
        }
    }
    pub fn me() -> Self {
        Self::new(PlayerKind::Human, "나", "")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    HumanVsHuman,
    HumanVsLlm,
    LlmVsLlm,
    HumanVsStockfish,
    StockfishVsLlm,
}

impl GameMode {
    pub const ALL: [Self; 5] = [
        Self::HumanVsHuman,
        Self::HumanVsLlm,
        Self::LlmVsLlm,
        Self::HumanVsStockfish,
        Self::StockfishVsLlm,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::HumanVsHuman => "사람 vs 사람",
            Self::HumanVsLlm => "사람 vs LLM",
            Self::LlmVsLlm => "LLM vs LLM",
            Self::HumanVsStockfish => "사람 vs Stockfish",
            Self::StockfishVsLlm => "Stockfish vs LLM",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptProtocol {
    ArenaDirect,
    PaperBenchmark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    pub id: Option<Uuid>,
    pub name: String,
    pub kind: String,
    pub elo_before: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub response: String,
    pub classification: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveRecord {
    pub ply: u32,
    pub side: Side,
    pub uci: String,
    pub san: String,
    pub fen_before: String,
    pub fen_after: String,
    pub prompt: Option<String>,
    pub attempts: Vec<Attempt>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub mode: String,
    pub value: i32,
    pub move_time_ms: u32,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisRecord {
    pub ply: u32,
    pub best_move: String,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub depth: u32,
    pub quality: String,
    pub pv: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    pub schema_version: u32,
    pub id: Uuid,
    pub mode: GameMode,
    pub protocol: PromptProtocol,
    pub white: Participant,
    pub black: Participant,
    pub rated: bool,
    pub started_at: f64,
    pub finished_at: Option<f64>,
    pub result: Option<String>,
    pub termination: Option<String>,
    pub initial_fen: String,
    pub current_fen: String,
    pub moves: Vec<MoveRecord>,
    pub engine: Option<EngineConfig>,
    pub review: Vec<AnalysisRecord>,
    pub coaching: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RatingEvent {
    pub game_id: Uuid,
    pub profile_id: Uuid,
    pub pool: String,
    pub before: f64,
    pub after: f64,
    pub opponent: f64,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppData {
    pub schema_version: u32,
    pub profiles: Vec<PlayerProfile>,
    pub games: Vec<GameRecord>,
    pub ratings: Vec<RatingEvent>,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profiles: vec![
                PlayerProfile::me(),
                PlayerProfile::new(PlayerKind::Llm, "LLM A", "수동 프롬프트 연결"),
                PlayerProfile::new(PlayerKind::Llm, "LLM B", "수동 프롬프트 연결"),
            ],
            games: vec![],
            ratings: vec![],
        }
    }
}
