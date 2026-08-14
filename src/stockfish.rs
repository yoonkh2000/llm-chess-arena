#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineLine {
    pub depth: u32,
    pub score_cp: Option<i32>,
    pub mate: Option<i32>,
    pub pv: Vec<String>,
}

pub fn parse_info(line: &str) -> Option<EngineLine> {
    if !line.starts_with("info ") || !line.contains(" pv ") {
        return None;
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut parsed = EngineLine::default();
    let mut index = 0;
    while index < parts.len() {
        match parts[index] {
            "depth" if index + 1 < parts.len() => {
                parsed.depth = parts[index + 1].parse().unwrap_or(0)
            }
            "cp" if index > 0 && parts[index - 1] == "score" && index + 1 < parts.len() => {
                parsed.score_cp = parts[index + 1].parse().ok()
            }
            "mate" if index > 0 && parts[index - 1] == "score" && index + 1 < parts.len() => {
                parsed.mate = parts[index + 1].parse().ok()
            }
            "pv" => {
                parsed.pv = parts[index + 1..]
                    .iter()
                    .map(|part| (*part).to_owned())
                    .collect();
                break;
            }
            _ => {}
        }
        index += 1;
    }
    Some(parsed)
}

pub fn parse_bestmove(line: &str) -> Option<String> {
    line.strip_prefix("bestmove ")?
        .split_whitespace()
        .next()
        .map(str::to_owned)
}

pub fn skill_level_for_elo(elo: i32) -> i32 {
    ((elo.clamp(800, 2800) - 800) / 100).clamp(0, 20)
}
