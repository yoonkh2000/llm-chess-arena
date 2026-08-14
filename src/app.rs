use js_sys::{Array, Date, Math};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    Blob, BlobPropertyBag, DragEvent, HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement,
    MessageEvent, Url, Worker,
};
use yew::prelude::*;

use crate::{
    chess::{ChessGame, START_FEN, pieces_from_fen, square_name, unicode_piece},
    model::{
        AnalysisRecord, AppData, Attempt, EngineConfig, GameMode, GameRecord, MoveRecord,
        Participant, PlayerKind, PlayerProfile, PromptProtocol, RatingEvent, SCHEMA_VERSION, Side,
        SidePreference,
    },
    prompt::{coaching_prompt, move_prompt, parse_llm_response, pgn},
    rating::{benchmark_estimate, updated_rating},
    stockfish::{EngineLine, parse_bestmove, parse_info},
    storage,
};

#[derive(Clone, Copy, PartialEq)]
pub enum View {
    Play,
    Games,
    Ratings,
    Review,
    Profiles,
    Data,
}

struct MatchState {
    chess: ChessGame,
    record: GameRecord,
    selected: Option<String>,
    pending_promotion: Option<PendingPromotion>,
    llm_input: String,
    notice: String,
    invalid_attempts: u8,
    pending_attempts: Vec<Attempt>,
    waiting_engine: bool,
}

#[derive(Clone)]
struct PendingPromotion {
    from: String,
    to: String,
    choices: Vec<char>,
}

pub struct App {
    data: AppData,
    view: View,
    loaded: bool,
    status: String,
    mode: GameMode,
    protocol: PromptProtocol,
    rated: bool,
    primary_side: SidePreference,
    human_one: String,
    human_two: String,
    llm_one: String,
    llm_two: String,
    quick_llm_name: String,
    engine_elo: i32,
    profile_kind: PlayerKind,
    profile_name: String,
    profile_model: String,
    import_text: String,
    active: Option<MatchState>,
    worker: Option<Worker>,
    _worker_callback: Option<Closure<dyn FnMut(MessageEvent)>>,
    review_game: Option<uuid::Uuid>,
    review_index: usize,
    review_cursor: usize,
    review_line: EngineLine,
    review_busy: bool,
    position_analysis: Option<EngineLine>,
    position_analysis_busy: bool,
}

pub enum Msg {
    Loaded(Result<AppData, String>),
    Navigate(View),
    SetMode(String),
    SetProtocol(String),
    SetRated(bool),
    SetSide(String),
    SetHumanOne(String),
    SetHumanTwo(String),
    SetLlmOne(String),
    SetLlmTwo(String),
    SetQuickLlmName(String),
    AddQuickLlm,
    SetEngineElo(String),
    StartGame,
    NewMatch,
    SelectSquare(String),
    DragStart(String),
    DropSquare(String),
    DragEnd,
    ChoosePromotion(char),
    CancelPromotion,
    SetLlmInput(String),
    SubmitLlm,
    Resign,
    AgreeDraw,
    AnalyzeCurrent,
    Engine(String),
    SetProfileKind(String),
    SetProfileName(String),
    SetProfileModel(String),
    AddProfile,
    ToggleProfile(uuid::Uuid),
    SetImport(String),
    Import,
    ExportJson,
    ExportPgn(uuid::Uuid),
    StartReview(uuid::Uuid),
    ReviewPrevious,
    ReviewNext,
    CopyCoach(uuid::Uuid, bool),
    SetCoaching(uuid::Uuid, String),
    Copy(String),
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(ctx: &Context<Self>) -> Self {
        let link = ctx.link().clone();
        spawn_local(async move {
            link.send_message(Msg::Loaded(storage::load().await));
        });

        let (worker, callback) = match Worker::new("stockfish/stockfish-18-lite-single.js") {
            Ok(worker) => {
                let link = ctx.link().clone();
                let callback =
                    Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                        if let Some(line) = event.data().as_string() {
                            link.send_message(Msg::Engine(line));
                        }
                    });
                worker.set_onmessage(Some(callback.as_ref().unchecked_ref()));
                let _ = worker.post_message(&JsValue::from_str("uci"));
                (Some(worker), Some(callback))
            }
            Err(_) => (None, None),
        };

        Self {
            data: AppData::default(),
            view: View::Play,
            loaded: false,
            status: "로컬 기록을 여는 중…".into(),
            mode: GameMode::HumanVsLlm,
            protocol: PromptProtocol::ArenaDirect,
            rated: true,
            primary_side: SidePreference::White,
            human_one: String::new(),
            human_two: String::new(),
            llm_one: String::new(),
            llm_two: String::new(),
            quick_llm_name: String::new(),
            engine_elo: 1500,
            profile_kind: PlayerKind::Llm,
            profile_name: String::new(),
            profile_model: String::new(),
            import_text: String::new(),
            active: None,
            worker,
            _worker_callback: callback,
            review_game: None,
            review_index: 0,
            review_cursor: 0,
            review_line: EngineLine::default(),
            review_busy: false,
            position_analysis: None,
            position_analysis_busy: false,
        }
    }

    fn update(&mut self, _ctx: &Context<Self>, msg: Self::Message) -> bool {
        let mut should_save = false;
        match msg {
            Msg::Loaded(result) => {
                self.loaded = true;
                match result {
                    Ok(data) => {
                        self.data = data;
                        self.status = "IndexedDB에 자동 저장됩니다.".into();
                        if let Some(record) = self
                            .data
                            .games
                            .iter()
                            .rev()
                            .find(|game| game.result.is_none())
                            .cloned()
                        {
                            let moves = record
                                .moves
                                .iter()
                                .filter(|mv| !mv.uci.is_empty())
                                .map(|mv| mv.uci.clone())
                                .collect::<Vec<_>>();
                            if let Ok(chess) = ChessGame::replay(&record.initial_fen, &moves) {
                                self.active = Some(MatchState {
                                    chess,
                                    record,
                                    selected: None,
                                    pending_promotion: None,
                                    llm_input: String::new(),
                                    notice: "저장된 대국을 이어갑니다.".into(),
                                    invalid_attempts: 0,
                                    pending_attempts: vec![],
                                    waiting_engine: false,
                                });
                            }
                        }
                    }
                    Err(error) => self.status = format!("저장소를 열지 못했습니다: {error}"),
                }
                self.fill_default_selections();
                self.request_engine_turn();
            }
            Msg::Navigate(view) => self.view = view,
            Msg::SetMode(value) => {
                if let Some(mode) = parse_mode(&value) {
                    self.mode = mode;
                }
            }
            Msg::SetProtocol(value) => {
                self.protocol = if value == "paper" {
                    PromptProtocol::PaperBenchmark
                } else {
                    PromptProtocol::ArenaDirect
                }
            }
            Msg::SetRated(value) => self.rated = value,
            Msg::SetSide(value) => {
                self.primary_side = match value.as_str() {
                    "black" => SidePreference::Black,
                    "random" => SidePreference::Random,
                    _ => SidePreference::White,
                }
            }
            Msg::SetHumanOne(value) => self.human_one = value,
            Msg::SetHumanTwo(value) => self.human_two = value,
            Msg::SetLlmOne(value) => self.llm_one = value,
            Msg::SetLlmTwo(value) => self.llm_two = value,
            Msg::SetQuickLlmName(value) => self.quick_llm_name = value,
            Msg::AddQuickLlm => {
                let name = self.quick_llm_name.trim().to_owned();
                if name.is_empty() {
                    self.status = "추가할 LLM 이름을 입력해 주세요.".into();
                } else {
                    let id = if let Some(profile) = self
                        .data
                        .profiles
                        .iter_mut()
                        .find(|profile| profile.kind == PlayerKind::Llm && profile.name == name)
                    {
                        profile.active = true;
                        profile.id
                    } else {
                        let profile =
                            PlayerProfile::new(PlayerKind::Llm, &name, "대국 설정에서 추가");
                        let id = profile.id;
                        self.data.profiles.push(profile);
                        id
                    };
                    self.llm_one = id.to_string();
                    self.quick_llm_name.clear();
                    self.status = format!("LLM 프로필 ‘{name}’을 선택했습니다.");
                    should_save = true;
                }
            }
            Msg::SetEngineElo(value) => {
                if let Ok(elo) = value.parse() {
                    self.engine_elo = elo;
                }
            }
            Msg::StartGame => match self.create_match() {
                Ok(state) => {
                    self.data.games.push(state.record.clone());
                    self.active = Some(state);
                    self.position_analysis = None;
                    self.position_analysis_busy = false;
                    self.status = "대국을 시작했습니다.".into();
                    self.request_engine_turn();
                    should_save = true;
                }
                Err(error) => self.status = error,
            },
            Msg::NewMatch => {
                if self
                    .active
                    .as_ref()
                    .is_none_or(|active| active.record.result.is_some())
                {
                    self.active = None;
                    self.position_analysis = None;
                    self.position_analysis_busy = false;
                    self.status = "새 대국을 설정하세요.".into();
                }
            }
            Msg::SelectSquare(square) => {
                if self.select_human_square(square) {
                    should_save = true;
                }
            }
            Msg::DragStart(square) => {
                if self.position_analysis_busy {
                    self.status = "현재 포지션 분석이 끝날 때까지 잠시 기다려 주세요.".into();
                    return true;
                }
                if let Some(active) = self.active.as_mut()
                    && active.record.result.is_none()
                    && participant_kind(active) == "human"
                    && active.pending_promotion.is_none()
                    && active
                        .chess
                        .legal_moves()
                        .iter()
                        .any(|uci| uci.starts_with(&square))
                {
                    active.selected = Some(square);
                }
            }
            Msg::DropSquare(square) => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|active| active.selected.is_some())
                    && self.select_human_square(square)
                {
                    should_save = true;
                }
            }
            Msg::DragEnd => {
                if let Some(active) = self.active.as_mut() {
                    active.selected = None;
                }
            }
            Msg::ChoosePromotion(choice) => {
                let mut moved = false;
                if let Some(active) = self.active.as_mut()
                    && let Some(pending) = active.pending_promotion.take()
                {
                    if pending.choices.contains(&choice) {
                        let uci = format!("{}{}{}", pending.from, pending.to, choice);
                        match play_move(active, &uci, None, vec![]) {
                            Ok(()) => moved = true,
                            Err(error) => active.notice = error,
                        }
                    } else {
                        active.pending_promotion = Some(pending);
                    }
                }
                if moved {
                    self.finish_human_move();
                    should_save = true;
                }
            }
            Msg::CancelPromotion => {
                if let Some(active) = self.active.as_mut() {
                    active.pending_promotion = None;
                    active.selected = None;
                    active.notice = "승격 선택을 취소했습니다.".into();
                }
            }
            Msg::SetLlmInput(value) => {
                if let Some(active) = self.active.as_mut() {
                    active.llm_input = value;
                }
            }
            Msg::SubmitLlm => {
                if self.position_analysis_busy {
                    self.status = "현재 포지션 분석이 끝날 때까지 잠시 기다려 주세요.".into();
                    return true;
                }
                if let Some(active) = self.active.as_mut() {
                    if active.record.result.is_some() || participant_kind(active) != "llm" {
                        return false;
                    }
                    let response = active.llm_input.trim().to_owned();
                    let started = Date::now();
                    match parse_llm_response(&response) {
                        Ok(parsed) => {
                            let prompt = move_prompt(
                                &active.record,
                                active.chess.side_to_move(),
                                &active.chess.legal_moves(),
                            );
                            let attempt = Attempt {
                                response: response.clone(),
                                classification: parsed.classification,
                                elapsed_ms: (Date::now() - started) as u64,
                            };
                            let mut attempts = std::mem::take(&mut active.pending_attempts);
                            attempts.push(attempt);
                            match play_move(active, &parsed.uci, Some(prompt), attempts.clone()) {
                                Ok(()) => {
                                    self.position_analysis = None;
                                    active.llm_input.clear();
                                    active.invalid_attempts = 0;
                                    self.sync_active_record();
                                    should_save = true;
                                    self.finish_active_if_needed();
                                    self.request_engine_turn();
                                }
                                Err(error) => {
                                    attempts.pop();
                                    active.pending_attempts = attempts;
                                    self.register_invalid_llm(error, response);
                                    should_save = true;
                                }
                            }
                        }
                        Err(error) => {
                            self.register_invalid_llm(error.into(), response);
                            should_save = true;
                        }
                    }
                }
            }
            Msg::Resign => {
                if let Some(active) = self.active.as_mut()
                    && active.record.result.is_none()
                {
                    let result = if active.chess.side_to_move() == Side::White {
                        "0-1"
                    } else {
                        "1-0"
                    };
                    set_result(active, result, "기권");
                    should_save = true;
                }
                if should_save {
                    self.finish_rating();
                }
            }
            Msg::AgreeDraw => {
                if let Some(active) = self.active.as_mut()
                    && active.record.result.is_none()
                {
                    set_result(active, "1/2-1/2", "합의 무승부");
                    should_save = true;
                }
                if should_save {
                    self.finish_rating();
                }
            }
            Msg::AnalyzeCurrent => {
                let fen = self
                    .active
                    .as_ref()
                    .and_then(|active| (!active.waiting_engine).then(|| active.chess.fen()));
                if let Some(fen) = fen {
                    self.position_analysis = None;
                    self.position_analysis_busy = true;
                    self.status = "요청한 현재 포지션을 Stockfish가 분석 중입니다.".into();
                    self.engine_commands(&[format!("position fen {fen}"), "go depth 14".into()]);
                } else {
                    self.status =
                        "Stockfish가 착수 중일 때는 별도 분석을 시작할 수 없습니다.".into();
                }
            }
            Msg::Engine(line) => {
                if let Some(info) = parse_info(&line) {
                    if self.review_busy {
                        self.review_line = info;
                    } else if self.position_analysis_busy {
                        self.position_analysis = Some(info);
                    }
                }
                if let Some(best) = parse_bestmove(&line) {
                    if self.review_busy {
                        self.accept_review_bestmove(best);
                        should_save = true;
                    } else if self.position_analysis_busy {
                        self.position_analysis_busy = false;
                        if let Some(analysis) = self.position_analysis.as_mut()
                            && analysis.pv.is_empty()
                        {
                            analysis.pv.push(best);
                        }
                        self.status = "요청한 현재 포지션 분석을 완료했습니다.".into();
                    } else if let Some(active) = self.active.as_mut()
                        && active.waiting_engine
                        && active.record.result.is_none()
                    {
                        active.waiting_engine = false;
                        if play_move(active, &best, None, vec![]).is_ok() {
                            self.position_analysis = None;
                            self.sync_active_record();
                            should_save = true;
                            self.finish_active_if_needed();
                            self.request_engine_turn();
                        }
                    }
                }
            }
            Msg::SetProfileKind(value) => {
                self.profile_kind = if value == "human" {
                    PlayerKind::Human
                } else {
                    PlayerKind::Llm
                }
            }
            Msg::SetProfileName(value) => self.profile_name = value,
            Msg::SetProfileModel(value) => self.profile_model = value,
            Msg::AddProfile => {
                if !self.profile_name.trim().is_empty() {
                    self.data.profiles.push(PlayerProfile::new(
                        self.profile_kind,
                        self.profile_name.trim(),
                        self.profile_model.trim(),
                    ));
                    self.profile_name.clear();
                    self.profile_model.clear();
                    self.fill_default_selections();
                    should_save = true;
                }
            }
            Msg::ToggleProfile(id) => {
                if let Some(profile) = self
                    .data
                    .profiles
                    .iter_mut()
                    .find(|profile| profile.id == id)
                {
                    profile.active = !profile.active;
                    should_save = true;
                }
            }
            Msg::SetImport(value) => self.import_text = value,
            Msg::Import => match storage::import_json(&self.import_text) {
                Ok(data) => {
                    self.data = data;
                    self.active = None;
                    self.status = "백업을 복원했습니다.".into();
                    self.fill_default_selections();
                    should_save = true;
                }
                Err(error) => self.status = format!("복원 실패: {error}"),
            },
            Msg::ExportJson => match storage::export_json(&self.data) {
                Ok(json) => download("llm-chess-arena-backup.json", "application/json", &json),
                Err(error) => self.status = format!("백업 생성 실패: {error}"),
            },
            Msg::ExportPgn(id) => {
                if let Some(game) = self.data.games.iter().find(|game| game.id == id) {
                    download(
                        &format!("game-{id}.pgn"),
                        "application/x-chess-pgn",
                        &pgn(game),
                    );
                }
            }
            Msg::StartReview(id) => {
                if self.position_analysis_busy {
                    self.status = "현재 포지션 분석이 끝난 뒤 전체 리뷰를 시작해 주세요.".into();
                } else if self
                    .active
                    .as_ref()
                    .is_some_and(|active| active.record.result.is_none())
                {
                    self.status = "진행 중인 대국을 먼저 끝내 주세요.".into();
                } else {
                    self.review_game = Some(id);
                    self.review_index = 0;
                    self.review_cursor = 0;
                    self.review_line = EngineLine::default();
                    self.review_busy = true;
                    self.position_analysis = None;
                    self.view = View::Review;
                    if let Some(game) = self.data.games.iter_mut().find(|game| game.id == id) {
                        game.review.clear();
                    }
                    self.request_review_position();
                }
            }
            Msg::ReviewPrevious => {
                self.review_cursor = self.review_cursor.saturating_sub(1);
            }
            Msg::ReviewNext => {
                if let Some(game) = self
                    .review_game
                    .and_then(|id| self.data.games.iter().find(|game| game.id == id))
                {
                    let available = game.review.len().min(game.moves.len());
                    if self.review_cursor + 1 < available {
                        self.review_cursor += 1;
                    }
                }
            }
            Msg::CopyCoach(id, engine) => {
                if let Some(game) = self.data.games.iter().find(|game| game.id == id) {
                    copy_text(&coaching_prompt(game, engine));
                    self.status = "코칭 프롬프트를 클립보드에 복사했습니다.".into();
                }
            }
            Msg::SetCoaching(id, response) => {
                if let Some(game) = self.data.games.iter_mut().find(|game| game.id == id) {
                    game.coaching = (!response.trim().is_empty()).then_some(response);
                    should_save = true;
                }
            }
            Msg::Copy(value) => {
                copy_text(&value);
                self.status = "클립보드에 복사했습니다.".into();
            }
        }

        if should_save {
            let data = self.data.clone();
            spawn_local(async move {
                let _ = storage::save(&data).await;
            });
        }
        true
    }

    fn view(&self, ctx: &Context<Self>) -> Html {
        let nav = |view, label| {
            let active = self.view == view;
            html! { <button class={classes!("nav-button", active.then_some("active"))} onclick={ctx.link().callback(move |_| Msg::Navigate(view))}>{label}</button> }
        };
        html! {
            <div class="app-shell">
                <header class="topbar">
                    <div><span class="eyebrow">{"LOCAL-FIRST · RUST · WASM"}</span><h1>{"LLM Chess Arena"}</h1></div>
                    <a class="doc-link" href="docs/design/ko/" target="_blank">{"설계 문서 ↗"}</a>
                </header>
                <nav>{nav(View::Play, "대국")}{nav(View::Games, "기록")}{nav(View::Ratings, "Elo")}{nav(View::Review, "리뷰·코칭")}{nav(View::Profiles, "프로필")}{nav(View::Data, "데이터")}</nav>
                <div class="status"><span class={classes!("status-dot", self.loaded.then_some("ready"))}></span>{&self.status}</div>
                <main>{match self.view {
                    View::Play => self.view_play(ctx), View::Games => self.view_games(ctx), View::Ratings => self.view_ratings(),
                    View::Review => self.view_review(ctx), View::Profiles => self.view_profiles(ctx), View::Data => self.view_data(ctx),
                }}</main>
                <footer>{"모든 프로필·대국·Elo는 이 브라우저의 IndexedDB에 저장됩니다. 서버나 API 키가 필요 없습니다."}</footer>
            </div>
        }
    }
}

impl App {
    fn select_human_square(&mut self, square: String) -> bool {
        if self.position_analysis_busy {
            self.status = "현재 포지션 분석이 끝날 때까지 잠시 기다려 주세요.".into();
            return false;
        }
        let mut moved = false;
        if let Some(active) = self.active.as_mut() {
            if active.record.result.is_some() || participant_kind(active) != "human" {
                return false;
            }
            if active.pending_promotion.is_some() {
                active.notice = "먼저 승격할 기물을 선택하거나 취소해 주세요.".into();
                return false;
            }
            if let Some(from) = active.selected.take() {
                let uci = format!("{from}{square}");
                let legal_moves = active.chess.legal_moves();
                let choices = ['q', 'r', 'b', 'n']
                    .into_iter()
                    .filter(|piece| legal_moves.contains(&format!("{uci}{piece}")))
                    .collect::<Vec<_>>();
                if !choices.is_empty() {
                    active.pending_promotion = Some(PendingPromotion {
                        from,
                        to: square,
                        choices,
                    });
                    active.notice = "승격할 기물을 선택하세요.".into();
                    return false;
                }
                if let Err(error) = play_move(active, &uci, None, vec![]) {
                    active.notice = error;
                    active.selected = Some(square);
                } else {
                    moved = true;
                }
            } else {
                active.selected = Some(square);
            }
        }
        if moved {
            self.finish_human_move();
        }
        moved
    }

    fn finish_human_move(&mut self) {
        self.position_analysis = None;
        self.sync_active_record();
        self.finish_active_if_needed();
        self.request_engine_turn();
    }

    fn fill_default_selections(&mut self) {
        let humans: Vec<String> = self
            .data
            .profiles
            .iter()
            .filter(|p| p.kind == PlayerKind::Human && p.active)
            .map(|p| p.id.to_string())
            .collect();
        let llms: Vec<String> = self
            .data
            .profiles
            .iter()
            .filter(|p| p.kind == PlayerKind::Llm && p.active)
            .map(|p| p.id.to_string())
            .collect();
        if self.human_one.is_empty() {
            self.human_one = humans.first().cloned().unwrap_or_default();
        }
        if self.human_two.is_empty() {
            self.human_two = humans
                .get(1)
                .cloned()
                .or_else(|| humans.first().cloned())
                .unwrap_or_default();
        }
        if self.llm_one.is_empty() {
            self.llm_one = llms.first().cloned().unwrap_or_default();
        }
        if self.llm_two.is_empty() {
            self.llm_two = llms
                .get(1)
                .cloned()
                .or_else(|| llms.first().cloned())
                .unwrap_or_default();
        }
    }

    fn profile(&self, id: &str, kind: PlayerKind) -> Result<&PlayerProfile, String> {
        let id = uuid::Uuid::parse_str(id).map_err(|_| "프로필을 선택해 주세요.".to_string())?;
        self.data
            .profiles
            .iter()
            .find(|p| p.id == id && p.kind == kind && p.active)
            .ok_or_else(|| "사용 가능한 프로필을 선택해 주세요.".into())
    }

    fn create_match(&self) -> Result<MatchState, String> {
        if matches!(
            self.mode,
            GameMode::HumanVsStockfish | GameMode::StockfishVsLlm
        ) && self.worker.is_none()
        {
            return Err("이 브라우저에서 Stockfish Worker를 시작하지 못했습니다.".into());
        }
        let human1 = || {
            self.profile(&self.human_one, PlayerKind::Human)
                .map(participant)
        };
        let human2 = || {
            self.profile(&self.human_two, PlayerKind::Human)
                .map(participant)
        };
        let llm1 = || {
            self.profile(&self.llm_one, PlayerKind::Llm)
                .map(participant)
        };
        let llm2 = || {
            self.profile(&self.llm_two, PlayerKind::Llm)
                .map(participant)
        };
        let stockfish = || Participant {
            id: None,
            name: format!("Stockfish 18 ({})", self.engine_elo),
            kind: "stockfish".into(),
            elo_before: Some(self.engine_elo as f64),
        };
        let primary_side = self.primary_side.resolve(Math::random());
        let (white, black) = match self.mode {
            GameMode::HumanVsHuman => orient(primary_side, human1()?, human2()?),
            GameMode::HumanVsLlm => orient(primary_side, human1()?, llm1()?),
            GameMode::LlmVsLlm => orient(primary_side, llm1()?, llm2()?),
            GameMode::HumanVsStockfish => orient(primary_side, human1()?, stockfish()),
            GameMode::StockfishVsLlm => orient(primary_side, llm1()?, stockfish()),
        };
        if white.id.is_some() && white.id == black.id {
            return Err("서로 다른 두 프로필을 선택해 주세요.".into());
        }
        let record = GameRecord {
            schema_version: SCHEMA_VERSION,
            id: uuid::Uuid::new_v4(),
            mode: self.mode,
            protocol: self.protocol,
            white,
            black,
            rated: self.rated,
            started_at: Date::now(),
            finished_at: None,
            result: None,
            termination: None,
            initial_fen: START_FEN.into(),
            current_fen: START_FEN.into(),
            moves: vec![],
            engine: matches!(
                self.mode,
                GameMode::HumanVsStockfish | GameMode::StockfishVsLlm
            )
            .then(|| EngineConfig {
                mode: "uci_elo".into(),
                value: self.engine_elo,
                move_time_ms: 500,
                version: "Stockfish 18 lite single".into(),
            }),
            review: vec![],
            coaching: None,
        };
        let notice = format!(
            "색상 배정 · 백: {} · 흑: {}",
            record.white.name, record.black.name
        );
        Ok(MatchState {
            chess: ChessGame::default(),
            record,
            selected: None,
            pending_promotion: None,
            llm_input: String::new(),
            notice,
            invalid_attempts: 0,
            pending_attempts: vec![],
            waiting_engine: false,
        })
    }

    fn register_invalid_llm(&mut self, error: String, response: String) {
        let mut must_finish = false;
        if let Some(active) = self.active.as_mut() {
            active.invalid_attempts += 1;
            active.notice = format!("무효 응답 {}/3: {error}", active.invalid_attempts);
            let side = active.chess.side_to_move();
            let prompt = move_prompt(&active.record, side, &active.chess.legal_moves());
            let attempt = Attempt {
                response,
                classification: "invalid".into(),
                elapsed_ms: 0,
            };
            active.pending_attempts.push(attempt);
            if active.record.protocol == PromptProtocol::PaperBenchmark
                && active.invalid_attempts >= 3
            {
                active.record.moves.push(MoveRecord {
                    ply: active.chess.ply() + 1,
                    side,
                    uci: String::new(),
                    san: String::new(),
                    fen_before: active.chess.fen(),
                    fen_after: active.chess.fen(),
                    prompt: Some(prompt),
                    attempts: std::mem::take(&mut active.pending_attempts),
                });
                let result = if side == Side::White { "0-1" } else { "1-0" };
                set_result(active, result, "LLM 무효 응답 3회");
                must_finish = true;
            }
        }
        if must_finish {
            self.finish_rating();
        } else {
            self.sync_active_record();
        }
    }

    fn finish_active_if_needed(&mut self) -> bool {
        let end = self
            .active
            .as_ref()
            .and_then(|active| active.chess.game_end());
        if let (Some(active), Some(end)) = (self.active.as_mut(), end) {
            set_result(active, &end.result, &end.termination);
            self.finish_rating();
            return true;
        }
        false
    }

    fn finish_rating(&mut self) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let mut record = active.record.clone();
        let already_finished = self
            .data
            .games
            .iter()
            .find(|game| game.id == record.id)
            .is_some_and(|game| game.result.is_some());
        if record.rated && !already_finished {
            let white_score = match record.result.as_deref() {
                Some("1-0") => 1.0,
                Some("0-1") => 0.0,
                _ => 0.5,
            };
            match record.mode {
                GameMode::HumanVsHuman | GameMode::LlmVsLlm => self.rate_pair(&record, white_score),
                GameMode::HumanVsLlm | GameMode::HumanVsStockfish => {
                    self.rate_human(&record, white_score)
                }
                GameMode::StockfishVsLlm => {}
            }
        }
        record.finished_at.get_or_insert_with(Date::now);
        if let Some(saved) = self.data.games.iter_mut().find(|game| game.id == record.id) {
            *saved = record;
        } else {
            self.data.games.push(record);
        }
    }

    fn sync_active_record(&mut self) {
        let Some(record) = self.active.as_ref().map(|active| active.record.clone()) else {
            return;
        };
        if let Some(saved) = self.data.games.iter_mut().find(|game| game.id == record.id) {
            *saved = record;
        } else {
            self.data.games.push(record);
        }
    }

    fn rate_pair(&mut self, record: &GameRecord, white_score: f64) {
        let (Some(white_id), Some(black_id)) = (record.white.id, record.black.id) else {
            return;
        };
        let Some(wi) = self.data.profiles.iter().position(|p| p.id == white_id) else {
            return;
        };
        let Some(bi) = self.data.profiles.iter().position(|p| p.id == black_id) else {
            return;
        };
        if wi == bi {
            return;
        }
        let (white, black) = if wi < bi {
            let (a, b) = self.data.profiles.split_at_mut(bi);
            (&mut a[wi], &mut b[0])
        } else {
            let (a, b) = self.data.profiles.split_at_mut(wi);
            (&mut b[0], &mut a[bi])
        };
        let (wb, bb) = (white.elo, black.elo);
        white.elo = updated_rating(wb, bb, white_score);
        black.elo = updated_rating(bb, wb, 1.0 - white_score);
        let pool = if record.mode == GameMode::LlmVsLlm {
            "arena"
        } else {
            "personal"
        };
        self.data.ratings.extend([
            RatingEvent {
                game_id: record.id,
                profile_id: white_id,
                pool: pool.into(),
                before: wb,
                after: white.elo,
                opponent: bb,
                score: white_score,
            },
            RatingEvent {
                game_id: record.id,
                profile_id: black_id,
                pool: pool.into(),
                before: bb,
                after: black.elo,
                opponent: wb,
                score: 1.0 - white_score,
            },
        ]);
    }

    fn rate_human(&mut self, record: &GameRecord, white_score: f64) {
        let (human, opponent, score) = if record.white.kind == "human" {
            (&record.white, &record.black, white_score)
        } else {
            (&record.black, &record.white, 1.0 - white_score)
        };
        let (Some(id), Some(opponent_elo)) = (human.id, opponent.elo_before) else {
            return;
        };
        if let Some(profile) = self.data.profiles.iter_mut().find(|p| p.id == id) {
            let before = profile.elo;
            profile.elo = updated_rating(before, opponent_elo, score);
            self.data.ratings.push(RatingEvent {
                game_id: record.id,
                profile_id: id,
                pool: "personal".into(),
                before,
                after: profile.elo,
                opponent: opponent_elo,
                score,
            });
        }
    }

    fn request_engine_turn(&mut self) {
        let commands = {
            let Some(active) = self.active.as_mut() else {
                return;
            };
            if active.record.result.is_some()
                || participant_kind(active) != "stockfish"
                || active.waiting_engine
            {
                return;
            }
            active.waiting_engine = true;
            active.notice = "Stockfish가 수를 계산 중입니다…".into();
            let target = active
                .record
                .engine
                .as_ref()
                .map(|e| e.value)
                .unwrap_or(1500)
                .clamp(1320, 3190);
            vec![
                "ucinewgame".into(),
                "setoption name UCI_LimitStrength value true".into(),
                format!("setoption name UCI_Elo value {target}"),
                format!("position fen {}", active.chess.fen()),
                "go movetime 500".into(),
            ]
        };
        self.engine_commands(&commands);
    }

    fn engine_commands(&self, commands: &[String]) {
        if let Some(worker) = &self.worker {
            for command in commands {
                let _ = worker.post_message(&JsValue::from_str(command));
            }
        }
    }

    fn request_review_position(&mut self) {
        let Some(id) = self.review_game else {
            return;
        };
        let Some(game) = self.data.games.iter().find(|game| game.id == id) else {
            self.review_busy = false;
            return;
        };
        let Some(mv) = game.moves.get(self.review_index) else {
            self.review_busy = false;
            self.status = "Stockfish 리뷰가 완료되었습니다.".into();
            return;
        };
        self.review_line = EngineLine::default();
        self.engine_commands(&[
            format!("position fen {}", mv.fen_before),
            "go depth 12".into(),
        ]);
    }

    fn accept_review_bestmove(&mut self, best: String) {
        let Some(id) = self.review_game else {
            self.review_busy = false;
            return;
        };
        let Some(game) = self.data.games.iter_mut().find(|game| game.id == id) else {
            self.review_busy = false;
            return;
        };
        let Some(mv) = game.moves.get(self.review_index) else {
            self.review_busy = false;
            return;
        };
        game.review.push(AnalysisRecord {
            ply: mv.ply,
            best_move: best.clone(),
            score_cp: self.review_line.score_cp,
            mate: self.review_line.mate,
            depth: self.review_line.depth,
            quality: if mv.uci == best {
                "최선"
            } else {
                "검토 필요"
            }
            .into(),
            pv: self.review_line.pv.clone(),
        });
        self.review_index += 1;
        if self.review_index >= game.moves.len() {
            self.review_busy = false;
            self.status = "Stockfish 리뷰가 완료되었습니다.".into();
        } else {
            self.request_review_position();
        }
    }

    fn view_play(&self, ctx: &Context<Self>) -> Html {
        if let Some(active) = &self.active {
            return self.view_board(ctx, active);
        }
        let profile_options = |kind: PlayerKind, selected: &str| {
            self.data.profiles.iter().filter(move |p| p.kind == kind && p.active).map(|p| html! { <option value={p.id.to_string()} selected={p.id.to_string()==selected}>{format!("{} · Elo {:.0}{}", p.name, p.elo, if p.model.is_empty() { "".into() } else { format!(" · {}", p.model) })}</option> }).collect::<Html>()
        };
        let needs_h2 = self.mode == GameMode::HumanVsHuman;
        let needs_h1 = matches!(
            self.mode,
            GameMode::HumanVsHuman | GameMode::HumanVsLlm | GameMode::HumanVsStockfish
        );
        let needs_l1 = matches!(
            self.mode,
            GameMode::HumanVsLlm | GameMode::LlmVsLlm | GameMode::StockfishVsLlm
        );
        let needs_l2 = self.mode == GameMode::LlmVsLlm;
        html! { <section class="grid two">
            <div class="card hero-card"><span class="eyebrow">{"NEW MATCH"}</span><h2>{"새 대국"}</h2><p>{"사람, 외부 LLM, 브라우저 내 Stockfish를 원하는 조합으로 연결합니다."}</p>
                <label>{"대국 유형"}<select onchange={ctx.link().callback(|e: Event| Msg::SetMode(e.target_unchecked_into::<HtmlSelectElement>().value()))}>{for GameMode::ALL.map(|m| html!{<option value={mode_value(m)} selected={self.mode==m}>{m.label()}</option>})}</select></label>
                {if needs_h1 { html!{<label>{"사람 프로필"}<select onchange={ctx.link().callback(|e: Event| Msg::SetHumanOne(e.target_unchecked_into::<HtmlSelectElement>().value()))}>{profile_options(PlayerKind::Human,&self.human_one)}</select></label>} } else {html!{}}}
                {if needs_h2 { html!{<label>{"두 번째 사람"}<select onchange={ctx.link().callback(|e: Event| Msg::SetHumanTwo(e.target_unchecked_into::<HtmlSelectElement>().value()))}>{profile_options(PlayerKind::Human,&self.human_two)}</select></label>} } else {html!{}}}
                {if needs_l1 { html!{<><label>{"LLM 이름/프로필"}<select onchange={ctx.link().callback(|e: Event| Msg::SetLlmOne(e.target_unchecked_into::<HtmlSelectElement>().value()))}>{profile_options(PlayerKind::Llm,&self.llm_one)}</select></label><div class="quick-profile"><input aria-label="새 LLM 이름" value={self.quick_llm_name.clone()} placeholder="예: Opus 5 Extra Thinking" oninput={ctx.link().callback(|e:InputEvent|Msg::SetQuickLlmName(e.target_unchecked_into::<HtmlInputElement>().value()))}/><button onclick={ctx.link().callback(|_|Msg::AddQuickLlm)}>{"추가·선택"}</button></div></>} } else {html!{}}}
                {if needs_l2 { html!{<label>{"두 번째 LLM"}<select onchange={ctx.link().callback(|e: Event| Msg::SetLlmTwo(e.target_unchecked_into::<HtmlSelectElement>().value()))}>{profile_options(PlayerKind::Llm,&self.llm_two)}</select></label>} } else {html!{}}}
                <label>{"주 선수 색"}<select onchange={ctx.link().callback(|e: Event| Msg::SetSide(e.target_unchecked_into::<HtmlSelectElement>().value()))}><option value="white" selected={self.primary_side==SidePreference::White}>{"백"}</option><option value="black" selected={self.primary_side==SidePreference::Black}>{"흑"}</option><option value="random" selected={self.primary_side==SidePreference::Random}>{"랜덤"}</option></select></label>
                {if matches!(self.mode, GameMode::HumanVsStockfish | GameMode::StockfishVsLlm) {html!{<label>{format!("Stockfish 목표 Elo · {}", self.engine_elo)}<input type="range" min="1320" max="2800" step="100" value={self.engine_elo.to_string()} oninput={ctx.link().callback(|e: InputEvent| Msg::SetEngineElo(e.target_unchecked_into::<HtmlInputElement>().value()))}/></label>}} else {html!{}}}
                <div class="inline"><label>{"LLM 프로토콜"}<select onchange={ctx.link().callback(|e: Event| Msg::SetProtocol(e.target_unchecked_into::<HtmlSelectElement>().value()))}><option value="arena">{"간결 UCI"}</option><option value="paper">{"논문 벤치마크 (3회 제한)"}</option></select></label><label class="check"><input type="checkbox" checked={self.rated} onchange={ctx.link().callback(|e: Event| Msg::SetRated(e.target_unchecked_into::<HtmlInputElement>().checked()))}/>{"Elo 반영"}</label></div>
                <button class="primary" onclick={ctx.link().callback(|_| Msg::StartGame)}>{"대국 시작"}</button>
            </div>
            <div class="card"><span class="eyebrow">{"HOW IT WORKS"}</span><h2>{"외부 LLM과 두는 법"}</h2><ol><li>{"LLM 차례에 생성되는 프롬프트를 복사합니다."}</li><li>{"ChatGPT, Claude 등 원하는 LLM에 붙여넣습니다."}</li><li>{"응답을 다시 붙여넣으면 WASM 체스 코어가 합법 수인지 검증합니다."}</li></ol><p class="callout">{"API 키와 서버가 없습니다. 대국 데이터는 브라우저 IndexedDB에만 남습니다."}</p><a href="docs/design/ko/" target="_blank">{"전체 설계 읽기 →"}</a></div>
        </section> }
    }

    fn view_board(&self, ctx: &Context<Self>, active: &MatchState) -> Html {
        let pieces = pieces_from_fen(&active.chess.fen());
        let kind = participant_kind(active);
        let last_move = active
            .record
            .moves
            .iter()
            .rev()
            .find(|mv| mv.uci.len() >= 4);
        let last_squares = last_move.and_then(|mv| Some((mv.uci.get(0..2)?, mv.uci.get(2..4)?)));
        let prompt = (kind == "llm" && active.record.result.is_none()).then(|| {
            move_prompt(
                &active.record,
                active.chess.side_to_move(),
                &active.chess.legal_moves(),
            )
        });
        html! { <section class="match-layout">
            <div class="card board-card"><div class="player black"><b>{&active.record.black.name}</b><span>{active.record.black.elo_before.map(|e| format!("Elo {e:.0}")).unwrap_or_default()}</span></div>
                <div class="chessboard">{for (0..64).map(|index| {
                    let square=square_name(index);
                    let file=((b'a'+(index%8) as u8)as char).to_string();
                    let selected=active.selected.as_deref()==Some(&square);
                    let last_from=last_squares.is_some_and(|(from,_)|from==square);
                    let last_to=last_squares.is_some_and(|(_,to)|to==square);
                    let piece=pieces[index];
                    let glyph=piece.map(unicode_piece).unwrap_or("");
                    let piece_class=piece.map(|value|if value.is_ascii_uppercase(){"piece-white"}else{"piece-black"});
                    let movable=kind=="human" && active.record.result.is_none() && active.pending_promotion.is_none() && piece.is_some_and(|value|match active.chess.side_to_move(){Side::White=>value.is_ascii_uppercase(),Side::Black=>value.is_ascii_lowercase()});
                    let click_square=square.clone();
                    let drag_square=square.clone();
                    let drop_square=square.clone();
                    html!{<button class={classes!("square", ((index+index/8)%2==0).then_some("light"), ((index+index/8)%2!=0).then_some("dark"), last_from.then_some("last-from"), last_to.then_some("last-to"), selected.then_some("selected"))} aria-label={square} draggable={movable.to_string()} onclick={ctx.link().callback(move |_| Msg::SelectSquare(click_square.clone()))} ondragstart={ctx.link().callback(move |_:DragEvent|Msg::DragStart(drag_square.clone()))} ondragend={ctx.link().callback(|_:DragEvent|Msg::DragEnd)} ondragover={Callback::from(|event:DragEvent|event.prevent_default())} ondrop={ctx.link().callback(move |event:DragEvent|{event.prevent_default();Msg::DropSquare(drop_square.clone())})}><span class={classes!("piece",piece_class)}>{glyph}</span>{if index%8==0 {html!{<small class="rank-label">{format!("{}", 8-index/8)}</small>}} else {html!{}}}{if index/8==7 {html!{<small class="file-label">{file}</small>}} else {html!{}}}</button>}
                })}</div>
                <div class="player white"><b>{&active.record.white.name}</b><span>{active.record.white.elo_before.map(|e| format!("Elo {e:.0}")).unwrap_or_default()}</span></div>
            </div>
            <div class="side-stack"><div class="card"><span class="eyebrow">{active.record.mode.label()}</span><h2>{if let Some(result)=&active.record.result {format!("종료 · {result}")} else {format!("{} 차례", if active.chess.side_to_move()==Side::White {"백"} else {"흑"})}}</h2><p>{if active.notice.is_empty(){format!("{} ply · FEN은 매 수 자동 저장", active.chess.ply())}else{active.notice.clone()}}</p>{if let Some(promotion)=&active.pending_promotion{html!{<div class="promotion-picker"><span class="eyebrow">{"PROMOTION"}</span><h3>{"승격할 기물을 선택하세요"}</h3><p>{format!("{} → {}",promotion.from,promotion.to)}</p><div class="promotion-options">{for promotion.choices.iter().map(|choice|{let choice=*choice;let label=match choice{'q'=>"퀸",'r'=>"룩",'b'=>"비숍",'n'=>"나이트",_=>"기물"};let action=if choice=='n'{format!("{label}로 승격")}else{format!("{label}으로 승격")};html!{<button aria-label={action} onclick={ctx.link().callback(move |_|Msg::ChoosePromotion(choice))}><span>{unicode_piece(choice)}</span>{label}</button>}})}</div><button class="promotion-cancel" onclick={ctx.link().callback(|_|Msg::CancelPromotion)}>{"선택 취소"}</button></div>}}else{html!{}}}{if let (Some(mv),Some((from,to)))=(last_move,last_squares){html!{<p class="last-move-info"><span>{"마지막 이동"}</span><b>{format!("{from} → {to}")}</b><small>{&mv.san}</small></p>}}else{html!{}}}<div class="moves">{for active.record.moves.iter().filter(|m| !m.uci.is_empty()).map(|m| html!{<span>{format!("{}. {}", m.ply, m.san)}</span>})}</div><div class="inline"><button onclick={ctx.link().callback(|_| Msg::Resign)} disabled={active.record.result.is_some()}>{"기권"}</button><button onclick={ctx.link().callback(|_| Msg::AgreeDraw)} disabled={active.record.result.is_some()}>{"무승부"}</button>{if active.record.result.is_some(){html!{<button class="primary" onclick={ctx.link().callback(|_|Msg::NewMatch)}>{"새 대국"}</button>}}else{html!{}}}</div><div class="analysis-request"><button disabled={active.waiting_engine||self.position_analysis_busy||active.pending_promotion.is_some()} onclick={ctx.link().callback(|_|Msg::AnalyzeCurrent)}>{if self.position_analysis_busy{"Stockfish 분석 중…"}else{"현재 포지션 분석"}}</button>{if let Some(line)=&self.position_analysis{html!{<p class="analysis-result"><b>{line.pv.first().map(|mv|format!("추천 수 {mv}")).unwrap_or_else(||"분석 완료".into())}</b><span>{if let Some(mate)=line.mate{format!("메이트 {mate} · depth {}",line.depth)}else{format!("현재 차례 기준 {:+}cp · depth {}",line.score_cp.unwrap_or(0),line.depth)}}</span></p>}}else{html!{<small>{"누를 때만 로컬 Stockfish가 분석합니다."}</small>}}}</div></div>
                {if let Some(prompt)=prompt {
                    html!{<div class="card prompt-card"><span class="eyebrow">{"MANUAL LLM BRIDGE"}</span><h2>{"LLM 수 입력"}</h2><textarea class="prompt" readonly=true value={prompt.clone()} /><button onclick={ctx.link().callback(move |_| Msg::Copy(prompt.clone()))}>{"프롬프트 복사"}</button><textarea placeholder="LLM 응답: e2e4 또는 make_move(&quot;e2e4&quot;)" value={active.llm_input.clone()} oninput={ctx.link().callback(|e: InputEvent| Msg::SetLlmInput(e.target_unchecked_into::<HtmlTextAreaElement>().value()))} /><button class="primary" onclick={ctx.link().callback(|_| Msg::SubmitLlm)}>{"응답 검증 후 두기"}</button></div>}
                } else {
                    html!{<div class="card"><h2>{if kind=="stockfish" {"Stockfish 계산 중"} else if active.record.result.is_some() {"대국 완료"} else {"보드에서 수를 선택하세요"}}</h2><p>{if kind=="human" {"출발·도착 칸을 차례로 누르거나 말을 원하는 칸으로 드래그하세요."} else {"엔진 분석은 자동 표시하지 않으며, 직접 요청하거나 기록 리뷰에서만 실행됩니다."}}</p></div>}
                }}
            </div>
        </section> }
    }

    fn view_games(&self, ctx: &Context<Self>) -> Html {
        let games = self.data.games.iter().rev().map(|game| {
            let pgn_id = game.id;
            let review_id = game.id;
            html! {
                <article class="card game-row">
                    <div><b>{format!("{}  {}", game.white.name, game.result.as_deref().unwrap_or("*"))}</b><span>{format!("{} · {} · {}수", game.black.name, game.mode.label(), game.moves.iter().filter(|m|!m.uci.is_empty()).count())}</span></div>
                    <div class="inline"><button onclick={ctx.link().callback(move |_|Msg::ExportPgn(pgn_id))}>{"PGN"}</button><button onclick={ctx.link().callback(move |_|Msg::StartReview(review_id))}>{"Stockfish 리뷰"}</button></div>
                </article>
            }
        });
        html! { <section><div class="section-head"><div><span class="eyebrow">{"LOCAL ARCHIVE"}</span><h2>{"대국 기록"}</h2></div><span>{format!("{} games", self.data.games.len())}</span></div><div class="game-list">{if self.data.games.is_empty(){html!{<div class="empty">{"저장된 대국이 없습니다."}</div>}}else{html!{<>{for games}</>}}}</div></section> }
    }

    fn view_ratings(&self) -> Html {
        let mut profiles = self.data.profiles.clone();
        profiles.sort_by(|a, b| b.elo.total_cmp(&a.elo));
        let benchmark_rows: Vec<(String, Option<crate::rating::BenchmarkEstimate>)> = self
            .data
            .profiles
            .iter()
            .filter(|p| p.kind == PlayerKind::Llm)
            .map(|p| {
                let rows = self
                    .data
                    .games
                    .iter()
                    .filter(|g| {
                        g.mode == GameMode::StockfishVsLlm
                            && g.rated
                            && g.result.is_some()
                            && (g.white.id == Some(p.id) || g.black.id == Some(p.id))
                    })
                    .filter_map(|g| {
                        let engine = g.engine.as_ref()?;
                        let white = g.white.id == Some(p.id);
                        let ws = match g.result.as_deref()? {
                            "1-0" => 1.0,
                            "0-1" => 0.0,
                            _ => 0.5,
                        };
                        Some((
                            engine.value as f64,
                            if white { ws } else { 1.0 - ws },
                            white,
                        ))
                    })
                    .collect::<Vec<_>>();
                (p.name.clone(), benchmark_estimate(&rows))
            })
            .collect();
        html! {<section class="grid two"><div><span class="eyebrow">{"RATING POOLS"}</span><h2>{"프로필 Elo"}</h2><div class="leaderboard">{for profiles.iter().map(|p|html!{<div class="rank-row"><span>{if p.kind==PlayerKind::Human{"사람"}else{"LLM"}}</span><b>{&p.name}</b><strong>{format!("{:.0}",p.elo)}</strong></div>})}</div><p class="fine">{"사람 vs LLM/Stockfish에서는 사람 Elo만, LLM vs LLM에서는 두 LLM의 Arena Elo가 변합니다. K=32."}</p></div><div><span class="eyebrow">{"PAPER BENCHMARK"}</span><h2>{"Stockfish 기준 Rating"}</h2><div class="leaderboard">{for benchmark_rows.iter().map(|(name,estimate)|html!{<div class="rank-row"><span>{"95% CI"}</span><b>{name}</b><strong>{estimate.as_ref().map(|e|format!("{:.0} ± {:.0}",e.rating,e.margin)).unwrap_or_else(||"5국 필요".into())}</strong></div>})}</div><p class="fine">{"논문의 최대우도 추정과 백 +35 보정을 적용합니다. 일반 Arena Elo와 분리된 지표입니다."}</p></div></section>}
    }

    fn view_review(&self, ctx: &Context<Self>) -> Html {
        let selected = self
            .review_game
            .and_then(|id| self.data.games.iter().find(|g| g.id == id));
        html! {<section><div class="section-head"><div><span class="eyebrow">{"OPT-IN ENGINE"}</span><h2>{"Stockfish 리뷰"}</h2></div>{if self.review_busy{html!{<div class="progress"><span></span>{format!("분석 중 · {}번째 수",self.review_index+1)}</div>}}else{html!{}}}</div><p>{"기록에서 리뷰를 요청한 경기만 분석합니다. 이전·다음으로 각 수 직전 포지션을 확인하세요."}</p>{if let Some(game)=selected{let first_id=game.id;let second_id=game.id;let response_id=game.id;html!{<div class="review-layout"><div>{self.view_review_position(ctx,game)}</div><div><span class="eyebrow">{"LLM COACH"}</span><h2>{"코칭 프롬프트"}</h2><p>{"완료된 대국을 원하는 LLM에게 보내 한국어 코칭을 받을 수 있습니다."}</p><div class="card"><textarea class="prompt tall" readonly=true value={coaching_prompt(game,!game.review.is_empty())} /><div class="inline"><button onclick={ctx.link().callback(move |_|Msg::CopyCoach(first_id,false))}>{"대국만 복사"}</button><button class="primary" disabled={game.review.is_empty()} onclick={ctx.link().callback(move |_|Msg::CopyCoach(second_id,true))}>{"엔진 리뷰 포함"}</button></div><label>{"LLM 코칭 답변 저장"}<textarea placeholder="받은 코칭 답변을 붙여넣으면 IndexedDB에 저장됩니다." value={game.coaching.clone().unwrap_or_default()} oninput={ctx.link().callback(move |e:InputEvent|Msg::SetCoaching(response_id,e.target_unchecked_into::<HtmlTextAreaElement>().value()))} /></label></div></div></div>}}else{html!{<div class="empty">{"대국 기록에서 ‘Stockfish 리뷰’를 선택하세요."}</div>}}}</section>}
    }

    fn view_review_position(&self, ctx: &Context<Self>, game: &GameRecord) -> Html {
        let cursor = self.review_cursor.min(game.moves.len().saturating_sub(1));
        let current = game.moves.get(cursor);
        let analysis = current.and_then(|mv| game.review.iter().find(|row| row.ply == mv.ply));
        let actual_squares = current.and_then(|mv| uci_squares(&mv.uci));
        let best_squares = analysis.and_then(|row| uci_squares(&row.best_move));
        let fen = current
            .map(|mv| mv.fen_before.as_str())
            .unwrap_or(&game.initial_fen);
        let pieces = pieces_from_fen(fen);
        let available = game.review.len().min(game.moves.len());
        html! {<div class="card review-position"><h3>{format!("{} vs {} · {}",game.white.name,game.black.name,game.result.as_deref().unwrap_or("*"))}</h3><div class="review-nav"><button aria-label="이전 수" disabled={cursor==0} onclick={ctx.link().callback(|_|Msg::ReviewPrevious)}>{"← 이전"}</button><b>{if game.moves.is_empty(){"수 없음".into()}else{format!("{} / {}",cursor+1,game.moves.len())}}</b><button aria-label="다음 수" disabled={cursor+1>=available} onclick={ctx.link().callback(|_|Msg::ReviewNext)}>{"다음 →"}</button></div><div class="chessboard review-board">{for (0..64).map(|index|{let square=square_name(index);let file=((b'a'+(index%8)as u8)as char).to_string();let piece=pieces[index];let glyph=piece.map(unicode_piece).unwrap_or("");let piece_class=piece.map(|value|if value.is_ascii_uppercase(){"piece-white"}else{"piece-black"});let actual_from=actual_squares.is_some_and(|(from,_)|from==square);let actual_to=actual_squares.is_some_and(|(_,to)|to==square);let best_from=best_squares.is_some_and(|(from,_)|from==square);let best_to=best_squares.is_some_and(|(_,to)|to==square);html!{<div class={classes!("square","review-square",((index+index/8)%2==0).then_some("light"),((index+index/8)%2!=0).then_some("dark"),actual_from.then_some("actual-from"),actual_to.then_some("actual-to"),best_from.then_some("best-from"),best_to.then_some("best-to"))} aria-label={format!("리뷰 {square}")}><span class={classes!("piece",piece_class)}>{glyph}</span>{if index%8==0{html!{<small class="rank-label">{format!("{}",8-index/8)}</small>}}else{html!{}}}{if index/8==7{html!{<small class="file-label">{file}</small>}}else{html!{}}}</div>}})}</div><div class="review-legend"><span class="actual-key">{"실제 이동"}</span><span class="best-key">{"Stockfish 추천"}</span></div>{if let Some(mv)=current{html!{<div class="review-comparison"><div><span>{"실제 수"}</span><b>{actual_squares.map(|(from,to)|format!("{from} → {to}")).unwrap_or_else(||mv.uci.clone())}</b><small>{format!("{} · {}",mv.san,analysis.map(|row|row.quality.as_str()).unwrap_or("분석 중"))}</small></div><div class="recommendation"><span>{"더 좋은 수"}</span>{if let Some(row)=analysis{html!{<><b>{best_squares.map(|(from,to)|format!("{from} → {to}")).unwrap_or_else(||row.best_move.clone())}</b><small>{format!("depth {}{}",row.depth,row.score_cp.map(|score|format!(" · {score:+}cp")).unwrap_or_default())}</small></>}}else{html!{<b>{"Stockfish 분석 중…"}</b>}}}</div></div>}}else{html!{<div class="empty">{"기록된 수가 없습니다."}</div>}}}{if let Some(row)=analysis{html!{<p class="review-pv">{format!("추천 진행: {}",row.pv.join(" "))}</p>}}else{html!{}}}</div>}
    }

    fn view_profiles(&self, ctx: &Context<Self>) -> Html {
        let profiles = self.data.profiles.iter().map(|profile| {
            let id = profile.id;
            html! {
                <div class={classes!("card","profile",(!profile.active).then_some("inactive"))}>
                    <div><span>{if profile.kind==PlayerKind::Human{"사람"}else{"LLM"}}</span><b>{&profile.name}</b><small>{if profile.model.is_empty(){"개인 Elo"}else{&profile.model}}</small></div>
                    <strong>{format!("{:.0}",profile.elo)}</strong><button onclick={ctx.link().callback(move |_|Msg::ToggleProfile(id))}>{if profile.active{"비활성"}else{"활성"}}</button>
                </div>
            }
        });
        html! {<section class="grid two"><div><span class="eyebrow">{"PEOPLE & MODELS"}</span><h2>{"프로필"}</h2><div class="profile-list">{for profiles}</div></div><div class="card"><span class="eyebrow">{"ADD PROFILE"}</span><h2>{"새 프로필"}</h2><label>{"유형"}<select onchange={ctx.link().callback(|e:Event|Msg::SetProfileKind(e.target_unchecked_into::<HtmlSelectElement>().value()))}><option value="llm">{"LLM"}</option><option value="human">{"사람"}</option></select></label><label>{"이름"}<input value={self.profile_name.clone()} placeholder="예: GPT-5, 윤광호" oninput={ctx.link().callback(|e:InputEvent|Msg::SetProfileName(e.target_unchecked_into::<HtmlInputElement>().value()))}/></label><label>{"모델/메모"}<input value={self.profile_model.clone()} placeholder="예: gpt-5 / reasoning high" oninput={ctx.link().callback(|e:InputEvent|Msg::SetProfileModel(e.target_unchecked_into::<HtmlInputElement>().value()))}/></label><button class="primary" onclick={ctx.link().callback(|_|Msg::AddProfile)}>{"프로필 추가"}</button></div></section>}
    }

    fn view_data(&self, ctx: &Context<Self>) -> Html {
        html! {<section class="grid two"><div class="card"><span class="eyebrow">{"BACKUP"}</span><h2>{"게임 기록은 어디에 저장되나요?"}</h2><p>{"이 브라우저의 IndexedDB 데이터베이스 "}<code>{storage::DB_NAME}</code>{" 안에 프로필, 전체 수순, 프롬프트/응답, Elo 변동과 선택적 리뷰를 저장합니다."}</p><button class="primary" onclick={ctx.link().callback(|_|Msg::ExportJson)}>{"전체 JSON 백업 다운로드"}</button><p class="fine">{"브라우저 사이트 데이터 삭제 전 백업하세요. 다른 기기와 자동 동기화되지는 않습니다."}</p></div><div class="card"><span class="eyebrow">{"RESTORE"}</span><h2>{"JSON 복원"}</h2><textarea class="tall" placeholder="백업 JSON을 붙여넣으세요" value={self.import_text.clone()} oninput={ctx.link().callback(|e:InputEvent|Msg::SetImport(e.target_unchecked_into::<HtmlTextAreaElement>().value()))} /><button onclick={ctx.link().callback(|_|Msg::Import)}>{"검증 후 복원"}</button><p class="danger">{"현재 로컬 데이터가 백업 내용으로 교체됩니다."}</p></div></section>}
    }
}

fn participant(profile: &PlayerProfile) -> Participant {
    Participant {
        id: Some(profile.id),
        name: profile.name.clone(),
        kind: if profile.kind == PlayerKind::Human {
            "human"
        } else {
            "llm"
        }
        .into(),
        elo_before: Some(profile.elo),
    }
}
fn orient(
    primary_side: Side,
    primary: Participant,
    other: Participant,
) -> (Participant, Participant) {
    if primary_side == Side::White {
        (primary, other)
    } else {
        (other, primary)
    }
}

fn participant_kind(active: &MatchState) -> &str {
    if active.chess.side_to_move() == Side::White {
        &active.record.white.kind
    } else {
        &active.record.black.kind
    }
}

fn uci_squares(uci: &str) -> Option<(&str, &str)> {
    Some((uci.get(0..2)?, uci.get(2..4)?))
}

fn set_result(active: &mut MatchState, result: &str, termination: &str) {
    active.record.result = Some(result.into());
    active.record.termination = Some(termination.into());
    active.record.finished_at = Some(Date::now());
    active.notice = format!("{result} · {termination}");
}
fn play_move(
    active: &mut MatchState,
    uci: &str,
    prompt: Option<String>,
    attempts: Vec<Attempt>,
) -> Result<(), String> {
    let side = active.chess.side_to_move();
    let played = active.chess.play_uci(uci).map_err(|e| e.to_string())?;
    active.record.current_fen = played.fen_after.clone();
    active.record.moves.push(MoveRecord {
        ply: active.chess.ply(),
        side,
        uci: played.uci,
        san: played.san,
        fen_before: played.fen_before,
        fen_after: played.fen_after,
        prompt,
        attempts,
    });
    active.selected = None;
    active.notice.clear();
    Ok(())
}
fn mode_value(mode: GameMode) -> &'static str {
    match mode {
        GameMode::HumanVsHuman => "hvh",
        GameMode::HumanVsLlm => "hvl",
        GameMode::LlmVsLlm => "lvl",
        GameMode::HumanVsStockfish => "hvs",
        GameMode::StockfishVsLlm => "svl",
    }
}
fn parse_mode(value: &str) -> Option<GameMode> {
    Some(match value {
        "hvh" => GameMode::HumanVsHuman,
        "hvl" => GameMode::HumanVsLlm,
        "lvl" => GameMode::LlmVsLlm,
        "hvs" => GameMode::HumanVsStockfish,
        "svl" => GameMode::StockfishVsLlm,
        _ => return None,
    })
}

fn copy_text(text: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(text);
    }
}
fn download(filename: &str, mime: &str, content: &str) {
    let parts = Array::new();
    parts.push(&JsValue::from_str(content));
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    if let Ok(blob) = Blob::new_with_str_sequence_and_options(&parts, &options)
        && let Ok(url) = Url::create_object_url_with_blob(&blob)
    {
        if let Some(document) = web_sys::window().and_then(|w| w.document())
            && let Ok(element) = document.create_element("a")
            && let Ok(anchor) = element.dyn_into::<web_sys::HtmlAnchorElement>()
        {
            anchor.set_href(&url);
            anchor.set_download(filename);
            anchor.click();
        }
        let _ = Url::revoke_object_url(&url);
    }
}
