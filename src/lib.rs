pub mod chess;
pub mod model;
pub mod prompt;
pub mod rating;
pub mod stockfish;
pub mod storage;

#[cfg(target_arch = "wasm32")]
pub mod app;
