# LLM Chess Arena Design

**Date:** 2026-08-14

**Status:** Approved design awaiting implementation-plan review

**Target repository:** `yoonkh2000/llm-chess-arena`

**Target deployment:** `https://yoonkh2000.github.io/llm-chess-arena/`

## 1. Summary

LLM Chess Arena is a serverless Rust/Yew WebAssembly application for local chess games between human profiles, external LLMs, and a bundled Stockfish WebAssembly engine. It gives the user self-contained prompts to copy into any external LLM, accepts the LLM response by paste, validates every move locally, persists every game locally, calculates clearly separated rating types, and generates post-game coaching prompts.

The design is informed by *LLM CHESS: Benchmarking Reasoning and Instruction-Following in LLMs through Chess* ([arXiv:2512.01992](https://arxiv.org/pdf/2512.01992)). The product adopts the paper's UCI move contract, legal-move grounding, per-ply validation, failure metrics, fixed-opponent Elo estimation, confidence intervals, and optional Stockfish move-quality analysis. It deliberately adds a practical prompt mode, manual copy/paste operation, human play, dynamic arena ratings, local persistence, and coaching.

## 2. Goals

1. Run as a static website with no application server, account, API key, or network request required after the site assets are loaded.
2. Support five game modes:
   - Human vs Human
   - Human vs LLM
   - LLM vs LLM
   - Human vs Stockfish
   - Stockfish vs LLM
3. Generate reliable, self-contained prompts for external LLMs and parse pasted replies without allowing illegal moves to corrupt the game.
4. Autosave active and completed games in the current browser's IndexedDB.
5. Export and restore all application data as versioned JSON and export games as PGN.
6. Maintain an LLM-only Arena Elo, a local Personal Elo for human profiles, and paper-style benchmark ratings against fixed-Elo Stockfish opponents.
7. Keep Stockfish review disabled during play unless the user explicitly requests current-position analysis; provide full-game review only after the user starts it.
8. Generate external-LLM coaching prompts with or without a Stockfish review and store pasted coaching responses.
9. Publish source under AGPL-3.0-or-later to `yoonkh2000/llm-chess-arena` and deploy the verified static build to GitHub Pages.

## 3. Non-goals

- Calling OpenAI, Anthropic, Gemini, Ollama, or another LLM API from the app.
- Storing API keys or secrets.
- Cloud synchronization, authentication, shared accounts, or server-side backups.
- Real-time online multiplayer.
- Automatically controlling external LLM browser tabs.
- Claiming that Arena Elo or Personal Elo equals FIDE, Chess.com, or Lichess Elo.
- Showing an evaluation bar or a suggested move during ordinary play by default.
- Calibrating Stockfish `Skill Level` to a human Elo when the engine does not report a fixed `UCI_Elo` target.

## 4. Terminology and rating separation

- **LLM profile:** A locally named model identity, such as `Claude Opus 4.1` or `GPT-5 high`, used consistently across games.
- **Human profile:** A local person identity. The first launch creates one profile named `나` with a Personal Elo of 1200. Additional people can have independent profiles.
- **Arena Elo:** A dynamic, local rating pool changed only by rated LLM-vs-LLM games.
- **Personal Elo:** A dynamic, local rating for Human profiles. Human-vs-Human changes both participating Human ratings; other rated Human games change only the participating Human rating. Human games never change LLM Arena Elo or a Stockfish rating.
- **Benchmark Rating:** A maximum-likelihood rating with a 95% confidence interval derived only from games against Stockfish profiles that expose a fixed target Elo.
- **Stockfish opponent:** The engine automatically chooses a move only on its own turn and reveals no evaluation data.
- **Stockfish reviewer:** The engine evaluates a requested current position or an entire completed game and may reveal evaluations and alternatives.
- **Rated game:** A game explicitly marked as rated at creation time that ends normally or by an LLM invalid-response forfeit. Aborted, corrupt, engine-failed, or failed-save games are not rated.

## 5. Technology and delivery architecture

### 5.1 Stack

- Rust stable, with the repository toolchain pinned in `rust-toolchain.toml`.
- Yew 0.23 client-side rendering.
- Trunk for the Rust-to-WASM build, static asset bundling, local development, and release output.
- `shakmaty` 0.30.x for standard chess rules, legal move generation, FEN, SAN, and UCI.
- `serde` and `serde_json` for versioned persistence and exports.
- `rexie` 0.6.x as the asynchronous Rust wrapper around IndexedDB.
- `wasm-bindgen`, `web-sys`, and `js-sys` at browser boundaries.
- `stockfish` 18.0.8, specifically `stockfish-18-lite-single.js` and `stockfish-18-lite-single.wasm`, copied as pinned single-thread local assets.
- A small JavaScript Worker adapter for loading the engine and forwarding typed UCI messages. All application state and UI logic remain in Rust.
- CSS owned by the repository; no required runtime CDN or third-party UI service.

### 5.2 Runtime components

```text
Yew UI
  -> Match controller
       -> Chess domain (shakmaty)
       -> Prompt builder / response parser
       -> Rating service
       -> IndexedDB repository
       -> Stockfish bridge
            -> Dedicated Web Worker
                 -> Stockfish WebAssembly
```

The match controller is a deterministic state machine. UI components dispatch domain commands and render state; they do not mutate chess state or ratings directly. IndexedDB and Stockfish are behind interfaces so the core behavior can be tested natively without a browser.

### 5.3 Static deployment

The release consists of HTML, CSS, JavaScript loaders, the application WASM, Stockfish Worker/WASM/network assets, icons, and license notices. It is deployed below the `/llm-chess-arena/` path, so all asset URLs must honor Trunk's public URL. The selected Stockfish build is single-threaded to avoid depending on cross-origin-isolation response headers that GitHub Pages cannot configure per repository.

## 6. Chess domain and match state machine

### 6.1 Domain authority

`shakmaty` is the sole authority for move legality and the resulting position. The app never trusts a move because it came from an LLM, a UI gesture, an imported PGN, or Stockfish. Every move is converted in the context of the current position and rejected if illegal.

The domain tracks:

- Current standard-chess position and full FEN.
- Move list in UCI and SAN.
- Side to move.
- Halfmove and fullmove counters.
- Repetition hashes needed for threefold/fivefold adjudication.
- Check, checkmate, stalemate, insufficient material, and claimable/automatic draw state.
- Maximum benchmark length of 200 plies when Paper Benchmark mode is selected.

### 6.2 Match states

```text
Setup
  -> AwaitingHumanMove
  -> AwaitingPromptCopy
  -> AwaitingLlmResponse
  -> ValidatingMove
  -> AwaitingEngineMove
  -> PersistingPly
  -> Finished
  -> PersistingResult
```

Recoverable error substates retain the last valid position. An invalid LLM response returns to `AwaitingLlmResponse`; a Stockfish failure offers engine restart or game abort; a save failure blocks rating finalization and offers retry or JSON emergency export.

### 6.3 Game modes

#### Human vs Human

The user chooses two distinct active Human profiles, colors or random color assignment, and a rated flag. Both people play on the same board using click-click or drag-drop interaction. No LLM prompt is generated and Stockfish remains idle unless a player explicitly requests current-position analysis or starts a review after the game. A rated result updates both Human profiles in the Personal Elo ledger. A rematch can swap colors.

#### Human vs LLM

The human chooses a local Human profile, an LLM profile, color, prompt protocol, and rated flag. Human moves use click-click and drag-drop board interaction. The LLM turn uses the manual prompt bridge. A rated result changes only the Human profile's Personal Elo.

#### LLM vs LLM

The user chooses two distinct LLM profiles, colors or random color assignment, prompt protocol, and rated flag. The UI always names the active LLM and gives only that player's prompt. A rematch can swap colors. A rated result updates both LLM profiles in the Arena Elo ledger.

#### Human vs Stockfish

The user chooses a Human profile, color, Stockfish strength mode, and rated flag. Stockfish moves automatically. A Target Elo result changes only Personal Elo; a Skill Level result is stored as unrated performance because it has no fixed rating anchor.

#### Stockfish vs LLM

The user chooses an LLM profile, color, prompt protocol, Stockfish strength mode, and benchmark flag. Stockfish moves automatically; LLM turns use copy/paste. Target Elo games feed the LLM's Benchmark Rating data but do not change Arena Elo. Skill Level games contribute only W/D/L and instruction-following statistics.

### 6.4 Completion and failure results

Normal results include checkmate, stalemate, insufficient material, automatic repetition/75-move draw, an accepted claimable repetition/50-move draw, resignation, and agreed draw. Administrative results include LLM invalid-response forfeit, engine failure, user abort, maximum 200-ply benchmark draw, and persistence failure.

Only normal results and LLM invalid-response forfeits can be rated. User abort, engine failure, and persistence failure are never rated. Every result stores a machine-readable termination reason and a Korean display label.

## 7. Manual LLM prompt bridge

### 7.1 General contract

Every turn prompt is self-contained so the user may paste it into a fresh external chat. The app never requests hidden chain-of-thought. It requests one action line and stores the entire pasted response for instruction-following analysis.

The requested response is:

```text
make_move <UCI>
```

The parser accepts exactly one unique legal UCI candidate from any of these forms:

```text
e7e5
make_move e7e5
{"move":"e7e5"}
```

If zero candidates or multiple distinct candidates are found, the response is unparseable. If one candidate is syntactically valid but illegal in the current position, it is an illegal move. The raw response, parser classification, extracted candidate, and attempt number are persisted.

### 7.2 Arena Direct prompt

Arena Direct is the default practical protocol. It contains:

- Player identity and assigned color.
- Full FEN.
- Coordinate-labelled ASCII board.
- Legal moves in UCI.
- Ordered move history in UCI and PGN/SAN.
- Game result rules and the strict one-line response contract.

The legal-move list is always included directly. The paper's ablations found direct information more reliable than forcing an agent to make extra tool calls, and manual copy/paste has no real tool-call channel.

### 7.3 Paper Benchmark prompt

Paper Benchmark treats each ply as an independent request. It contains the player color, full FEN, coordinate-labelled ASCII board, legal UCI moves, and strict response contract, but no previous move history. It applies a three-invalid-response forfeit and a 200-ply cap. Temperature and model reasoning settings cannot be controlled by this app, so the game record provides editable metadata fields for the user to record them.

### 7.4 Reflection prompts

An unparseable response produces a correction prompt containing the response contract, current FEN, and legal UCI list. An illegal move produces the same data plus the rejected UCI. Neither prompt changes the position.

Paper Benchmark forfeits the LLM after the third invalid response on one ply. Arena Direct shows `Try again`, `Forfeit`, and `Abort`; its default is to allow another attempt without an automatic limit, while still recording every failure.

### 7.5 Coaching prompt

The coaching prompt asks an external LLM to return concise Korean coaching with these sections:

1. Game summary.
2. Three turning points.
3. Tactical errors and missed opportunities.
4. Strategic or opening-pattern weakness.
5. Three concrete practice exercises.

It includes the selected player's identity, color, result, PGN, and that player's moves. If an engine review exists, it includes up to five largest mover-perspective win-probability losses, the Stockfish best move, principal variation, and engine version/settings. Without an engine review it states that the advice is LLM-only and must not claim engine verification. The generated prompt and pasted coaching answer are stored separately from the immutable game record.

## 8. Stockfish integration

### 8.1 Engine lifecycle

The Stockfish adapter owns a dedicated Worker and supports `boot`, `ready`, `set_options`, `search`, `stop`, `new_game`, and `terminate`. Searches are serialized. The app never changes options or positions during an active search. A request ID prevents a late response from an old or cancelled search from updating the current game or review.

On startup the bridge sends `uci`, parses the reported engine name/version and all supported option ranges, sends `isready`, and exposes capabilities to the Rust state. Unsupported controls are hidden rather than emulated.

### 8.2 Opponent mode

Stockfish opponent mode has two strength choices:

- **Target Elo:** Enable `UCI_LimitStrength` and set a `UCI_Elo` value within the range reported by the bundled engine.
- **Skill Level:** Disable `UCI_LimitStrength` and set `Skill Level` from the engine-reported range, expected to be 0 through 20.

The user also selects a per-move search budget: Fast 250 ms, Standard 1000 ms, or Deliberate 3000 ms. The exact engine name, engine version, build asset identifier, strength mode/value, search budget, Threads, and Hash are stored in the game record.

Opponent mode never sends evaluation lines to the visible UI. It waits for `bestmove`, validates the returned UCI against the current chess domain, persists it, and continues the match. An illegal or missing best move is an engine failure, not a loss by either player.

### 8.3 Review mode

Live analysis is off by default. The user can explicitly request `현재 포지션 분석` during a game or start a completed-game review. A current-position analysis does not persist unless the user saves it. A full review is stored as a replaceable `EngineReview` record and never mutates the original game.

Full review presets are:

- Fast: depth 10.
- Standard: depth 14.
- Precise: depth 18.

Each reviewed ply stores pre-move evaluation, post-move evaluation, mover-normalized evaluation, best move, principal variation, actual move, mate score if present, search depth, and engine identity. Review progress is resumable by ply and cancellable.

### 8.4 Move quality

Centipawn evaluation is normalized to White's perspective, then converted to the mover's perspective. For finite centipawn value `cp`, the paper's Lichess-derived win percentage is:

```text
Win% = 50 + 50 * (2 / (1 + exp(-0.00368208 * cp)) - 1)
```

For each move, `delta` is the mover's win percentage before the move minus after the move. Classifications are:

- Blunder: `delta >= 30`.
- Mistake: `20 <= delta < 30`.
- Inaccuracy: `10 <= delta < 20`.
- Best: actual UCI equals the engine's best UCI.
- Good: none of the above.

Mate scores are kept as mate scores and mapped to 100% or 0% only when a mover-perspective win probability is required. The review UI displays the underlying mate distance instead of a fabricated centipawn number.

## 9. Persistence and export

### 9.1 Storage location

The production database name is `llm-chess-arena-v1` under the origin `https://yoonkh2000.github.io`. Development origins such as `http://localhost:8080` have separate databases and do not automatically share records. GitHub stores application source and release assets only; it never receives the user's game data.

The app requests `navigator.storage.persist()` from Settings on an explicit user action and displays `Persistent`, `Best effort`, or `Unavailable`. It also displays usage/quota estimates when supported. Incognito/private browsing and explicit site-data deletion are clearly warned as destructive to local records.

### 9.2 Object stores

Database schema version 1 has these stores:

- `profiles`: `PlayerProfile` keyed by UUID.
- `games`: immutable completed `GameRecord` or mutable active-game snapshot keyed by UUID.
- `rating_events`: `RatingEvent` keyed by UUID and indexed by profile and completion time.
- `engine_reviews`: replaceable `EngineReview` keyed by review UUID and indexed by game UUID.
- `coaching_notes`: `CoachingNote` keyed by UUID and indexed by game UUID.
- `settings`: user settings and backup-reminder state keyed by string.

### 9.3 Main records

`PlayerProfile` stores UUID, kind (`human` or `llm`), display name, optional provider/model/reasoning metadata, creation time, active flag, and initial rating. Current ratings are derived from the ledger, not trusted as an independently editable value.

`GameRecord` stores UUID, schema version, mode, protocol, player snapshots, colors, rated/benchmark flags, start and finish times, result, termination reason, initial and final FEN, ordered plies, PGN, engine opponent settings if any, and save integrity state.

`PlyRecord` stores ply number, mover profile snapshot, FEN before and after, UCI, SAN, raw prompt if applicable, raw response if applicable, response attempts, elapsed manual-response time, and engine request metadata if Stockfish moved.

`RatingEvent` stores game UUID, rating pool, subject profile UUID, opponent snapshot/rating source, score, K-factor, expected score, rating before, rating after, and event time.

`EngineReview` stores game UUID, engine identity/settings, preset, creation time, completion state, last completed ply, and per-ply analyses.

`CoachingNote` stores game UUID, target profile UUID, prompt version, engine review UUID if used, generated prompt, pasted answer, language, and timestamps.

### 9.4 Autosave and atomicity

Setup is saved as soon as the game starts. Every accepted move is persisted in one transaction before the next actor is allowed to move. Finishing a rated game writes the final game and all rating events in one transaction. If that transaction fails, the UI retains the finished board in memory, does not display changed ratings, and offers retry plus emergency JSON export.

On launch, active games are listed as resumable. A resumed game reconstructs the position by replaying all stored UCI moves and verifies that every stored after-FEN matches. A mismatch marks the record corrupt and prevents further play while preserving export access.

### 9.5 Backup and PGN

Full backup files use the extension `.llmchess.json`, top-level `schema_version: 1`, export timestamp, application version, and arrays for all stores. Import validates the complete file before writing. Existing UUIDs are never silently overwritten; the user chooses `Skip conflicts` or `Cancel import`. A future schema version greater than the app supports is rejected without mutation.

PGN exports include standard tags plus `LLMChessArenaVersion`, `GameMode`, `PromptProtocol`, model metadata, Stockfish build/strength/search budget, and termination reason. Prompts and raw responses remain in JSON only so PGN stays interoperable and does not expose long chat content unexpectedly.

The header shows games added since the last full backup and displays a reminder after 10 completed games or 14 days, whichever comes first.

## 10. Rating algorithms

### 10.1 Arena Elo

Every LLM profile starts at 1200. For rated LLM-vs-LLM game score `S` and ratings `R_a`, `R_b`:

```text
E_a = 1 / (1 + 10 ^ ((R_b - R_a) / 400))
R_a' = R_a + 32 * (S_a - E_a)
R_b' = R_b + 32 * (S_b - (1 - E_a))
```

The stored ratings keep fractional precision; the UI rounds to the nearest integer. Win is 1, draw 0.5, loss 0. Color is not folded into the dynamic update. The leaderboard separately shows white and black records and promotes paired color-swapped matches.

Deleting a rated game or restoring a ledger rebuilds ratings chronologically using completion time and UUID as the deterministic tie-break. Rating events are regenerated and the old derived events are replaced transactionally.

### 10.2 Personal Elo

Every Human profile starts at 1200 and shares the local Personal Elo pool with other Human profiles. Personal Elo uses K=32.

- Against another Human profile, both profiles receive symmetric Personal Elo events using their pre-game Personal Elo ratings.
- Against an LLM, the opponent rating snapshot is that LLM's Arena Elo immediately before the game. Only the Human profile receives a Personal Elo event.
- Against Stockfish Target Elo, the opponent snapshot is the configured fixed target Elo. Only the Human profile receives a Personal Elo event.
- Against Stockfish Skill Level, no Personal Elo event is created.

Personal Elo is labelled as a local progress indicator, not an official chess rating. Deleting or importing games rebuilds it with the same deterministic ledger rules.

### 10.3 Fixed-opponent Benchmark Rating

For each eligible Target Elo game `i`, let opponent rating be `R_i`, player score be `S_i`, and color adjustment `C_i` be +35 when the rated subject played White and -35 when the subject played Black. For neutral player rating `R`:

```text
E_i(R) = 1 / (1 + 10 ^ ((R_i - (R + C_i)) / 400))
```

The estimate solves `sum(S_i - E_i(R)) = 0` by bisection within `[min(R_i)-400, max(R_i)+400]`. Fisher information and the confidence interval are:

```text
I(R) = sum(E_i(R) * (1 - E_i(R)) * (ln(10)/400)^2)
SE = 1 / sqrt(I(R))
95% CI = R +/- 1.96 * SE
```

The UI starts showing an estimate after five eligible games, labels fewer than 30 games `low sample`, and recommends at least 30 games per target strength. If all results force the root to a search boundary, the UI reports a boundary estimate and one-sided direction instead of pretending the estimate is well determined.

Benchmark Rating is available independently for LLM and Human profiles. It is computed from game records on demand and is not stored as mutable profile state.

## 11. User interface

### 11.1 Navigation

The responsive application has five top-level destinations:

- `Play`: create and continue games.
- `Games`: search active/completed records and export PGN/JSON.
- `Leaderboard`: LLM Arena Elo plus W/D/L, color splits, and invalid-response rate.
- `Review`: open a completed game, optionally run Stockfish, and create coaching prompts.
- `Settings`: profiles, storage status, backup/restore, defaults, licenses, and paper attribution.

Profile management creates, renames, and archives Human and LLM profiles. A profile referenced by a game cannot be physically deleted; archiving hides it from new-game selectors while preserving records and rating reconstruction. Two sides in Human-vs-Human and LLM-vs-LLM setup must use distinct active profile UUIDs.

### 11.2 Play layout

Desktop uses a two-column layout: board and player/turn status on the left, turn workbench on the right. Mobile stacks the board over the workbench. The workbench contains the active actor, prompt preview, `Copy prompt`, pasted-response textarea, validation result, retry/forfeit controls, move list, and save status. Version 1 is untimed; it records manual LLM response duration as a metric but does not adjudicate either Human or LLM by a chess clock.

Human moves support drag-drop and click-click. Keyboard users can select source and destination squares, and the board exposes piece/square labels to assistive technology. Promotion always opens an explicit choice. The board can flip to the active human or selected LLM perspective.

### 11.3 Games and ratings

Games can be filtered by participant, mode, result, protocol, rated status, and date. The details show all raw LLM interactions without placing them in PGN. The Leaderboard never mixes Human Personal Elo, LLM Arena Elo, and Benchmark Rating in one column; each has a separate label and explanation.

### 11.4 Review and coaching

Opening Review does not start the engine. The user can navigate plies immediately, then choose current-position analysis or a full review preset. During a review, progress and cancel are visible. After completion, the screen shows the move list, optional evaluation graph, quality labels, critical positions, and `Create coaching prompt`.

## 12. Error handling and privacy

- Invalid LLM output cannot advance the position.
- An outdated Stockfish response cannot affect a newer request.
- Stockfish crash restarts the Worker only after user confirmation and never assigns a player loss automatically.
- IndexedDB open/version/quota failures present a blocking storage status with in-memory emergency export.
- Import is validate-first and transactionally all-or-nothing.
- Clipboard denial falls back to selected text and manual copy.
- Unsupported persistent-storage or quota APIs degrade to an explanatory status.
- No prompts, responses, names, or games are sent to analytics, GitHub, or any remote endpoint.
- The application ships no telemetry.

## 13. Testing and verification

### 13.1 Native Rust tests

- Legal/illegal UCI parsing including castling, en passant, and promotion.
- Checkmate, stalemate, repetition, move-limit, and insufficient-material endings.
- Response parser acceptance and ambiguity rejection.
- Arena and Personal Elo numeric examples, draw symmetry, and chronological rebuild.
- Benchmark root finding, color adjustment, confidence interval, and boundary cases.
- Prompt snapshots proving Paper Benchmark excludes history and Arena Direct includes it.
- Serialization and schema validation.

### 13.2 Browser WASM tests

- IndexedDB create, autosave, resume, atomic finish, export, and conflict-safe restore.
- Clipboard fallback and downloadable JSON/PGN creation.
- Stockfish Worker boot, capability parsing, cancellation, request-ID isolation, and legal best move.
- Storage persistence/estimate capability fallbacks.

### 13.3 End-to-end browser tests

- Complete a Human-vs-LLM game from setup through saved result.
- Complete a rated Human-vs-Human game and verify symmetric Personal Elo events for both profiles.
- Complete a rated LLM-vs-LLM game and verify both rating events.
- Trigger three invalid Paper Benchmark responses and verify forfeit semantics.
- Complete Human-vs-Stockfish Target Elo and verify only Personal Elo changes.
- Complete Stockfish-vs-LLM and verify Arena Elo is unchanged and benchmark input exists.
- Export, clear a test database, restore, and reproduce game/rating views.
- Confirm Review remains idle until requested, can be cancelled, and stores a completed review.
- Generate coaching prompts with and without review evidence.
- Verify desktop and narrow mobile layouts, keyboard board operation, and page reload recovery.

### 13.4 Completion gates

- `cargo fmt --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Native Rust tests pass.
- Headless browser WASM tests pass.
- Release build under the GitHub Pages base path succeeds.
- The production-like build is inspected in a real browser with no console errors.
- A GitHub Pages deployment is not called complete until the public URL loads the expected commit and the WASM, Stockfish Worker, and IndexedDB flows are verified there.

## 14. CI, repository, deployment, and licensing

The repository default branch is `main`. GitHub Actions runs formatting, clippy, native tests, browser tests, and the release build. The Pages workflow publishes only after all required checks pass on `main`.

The approved design and later user-facing progress artifacts are rendered as navigable HTML, not delivered only as raw Markdown. The English design lives at `docs/design/index.html`, the complete Korean translation lives at `docs/design/ko/index.html`, and both pages provide a language switch. Local work is served over HTTP with a clickable preview URL, and the published repository links to both HTML designs from its README.

The repository is public and the application code uses AGPL-3.0-or-later. The pinned `stockfish` 18.0.8 package and bundled Stockfish.js assets use GPLv3, with their license text and source pointer distributed alongside the build. The repository includes the project license, exact npm package integrity, source/build links, third-party notices, and paper citation. The README is Korean-first with English setup commands and explains local-only data storage before the first-run instructions.

Publishing uses the authenticated `yoonkh2000` GitHub account. Before remote creation, the workflow checks whether `yoonkh2000/llm-chess-arena` already exists and never overwrites an existing remote. The local repository is committed and verified first; then it is pushed, GitHub Pages is enabled, and the public deployment is verified separately.

## 15. Acceptance scenarios

The first release is accepted when all of the following are true:

1. A user can use the default `나` profile, create another Human profile and two LLM profiles, and complete all five supported match types.
2. Every external-LLM turn provides a copyable self-contained prompt and accepts a valid pasted UCI response.
3. Illegal or ambiguous responses leave the board unchanged and are recorded.
4. Active games survive a normal page reload from IndexedDB.
5. LLM-vs-LLM changes only Arena Elo; Human-vs-Human changes both Human Personal Elo ratings; other rated Human games change only the participating Human rating; Target Elo games feed Benchmark Rating; Skill Level games do not claim an Elo anchor.
6. Stockfish opponent moves automatically without revealing analysis, while review remains opt-in.
7. A completed game can be exported as PGN, all data can be backed up/restored as versioned JSON, and conflicts cannot silently overwrite records.
8. A coaching prompt can be generated and its pasted answer saved both before and after an engine review.
9. The public repository and GitHub Pages site are owned by `yoonkh2000`, identify the deployed commit, and pass the completion gates.
