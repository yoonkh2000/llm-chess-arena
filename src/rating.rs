use crate::model::{GameMode, PlayerProfile, RatingEvent};
use uuid::Uuid;

pub fn expected_score(rating: f64, opponent: f64) -> f64 {
    1.0 / (1.0 + 10_f64.powf((opponent - rating) / 400.0))
}

pub fn updated_rating(rating: f64, opponent: f64, score: f64) -> f64 {
    rating + 32.0 * (score - expected_score(rating, opponent))
}

pub fn apply_pair(
    game_id: Uuid,
    pool: &str,
    a: &mut PlayerProfile,
    b: &mut PlayerProfile,
    a_score: f64,
) -> [RatingEvent; 2] {
    let (a_before, b_before) = (a.elo, b.elo);
    a.elo = updated_rating(a_before, b_before, a_score);
    b.elo = updated_rating(b_before, a_before, 1.0 - a_score);
    [
        RatingEvent {
            game_id,
            profile_id: a.id,
            pool: pool.into(),
            before: a_before,
            after: a.elo,
            opponent: b_before,
            score: a_score,
        },
        RatingEvent {
            game_id,
            profile_id: b.id,
            pool: pool.into(),
            before: b_before,
            after: b.elo,
            opponent: a_before,
            score: 1.0 - a_score,
        },
    ]
}

pub fn rating_pool(mode: GameMode) -> Option<&'static str> {
    match mode {
        GameMode::HumanVsHuman | GameMode::HumanVsLlm | GameMode::HumanVsStockfish => {
            Some("personal")
        }
        GameMode::LlmVsLlm => Some("arena"),
        GameMode::StockfishVsLlm => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkEstimate {
    pub rating: f64,
    pub margin: f64,
    pub sample_size: usize,
    pub boundary: bool,
}

pub fn benchmark_estimate(rows: &[(f64, f64, bool)]) -> Option<BenchmarkEstimate> {
    if rows.len() < 5 {
        return None;
    }
    let lo = rows.iter().map(|r| r.0).fold(f64::INFINITY, f64::min) - 400.0;
    let hi = rows.iter().map(|r| r.0).fold(f64::NEG_INFINITY, f64::max) + 400.0;
    let diff = |rating: f64| {
        rows.iter()
            .map(|(opp, score, white)| {
                let color = if *white { 35.0 } else { -35.0 };
                score - expected_score(rating + color, *opp)
            })
            .sum::<f64>()
    };
    let boundary = diff(lo).signum() == diff(hi).signum();
    let mut left = lo;
    let mut right = hi;
    for _ in 0..80 {
        let mid = (left + right) / 2.0;
        if diff(mid) > 0.0 {
            left = mid;
        } else {
            right = mid;
        }
    }
    let rating = if boundary {
        if diff(lo) > 0.0 { hi } else { lo }
    } else {
        (left + right) / 2.0
    };
    let k = std::f64::consts::LN_10 / 400.0;
    let info = rows
        .iter()
        .map(|(opp, _, white)| {
            let c = if *white { 35.0 } else { -35.0 };
            let e = expected_score(rating + c, *opp);
            e * (1.0 - e) * k * k
        })
        .sum::<f64>();
    Some(BenchmarkEstimate {
        rating,
        margin: 1.96 / info.sqrt(),
        sample_size: rows.len(),
        boundary,
    })
}
