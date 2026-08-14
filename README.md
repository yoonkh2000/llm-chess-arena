# LLM Chess Arena

사람·외부 LLM·Stockfish가 서로 체스를 둘 수 있는 로컬 우선 Rust/Yew WebAssembly 앱입니다. API 키나 서버 없이 외부 LLM에 프롬프트를 복사하고 응답을 붙여넣어 진행합니다.

## 기능

- 사람 vs 사람, 사람 vs LLM, LLM vs LLM, 사람 vs Stockfish, Stockfish vs LLM
- `shakmaty` 기반 로컬 합법 수 검증과 UCI/SAN/FEN 기록
- 사람 Personal Elo, LLM Arena Elo, Stockfish 기준 Benchmark Rating 및 95% 신뢰구간
- 브라우저 IndexedDB 자동 저장, 새로고침 후 이어두기, JSON 백업/복원, PGN 내보내기
- 요청할 때만 실행되는 Stockfish 18 리뷰와 외부 LLM용 한국어 코칭 프롬프트
- 한국어 앱과 [한국어 설계 문서](docs/design/ko/index.html), [English design document](docs/design/index.html)

게임 기록은 현재 브라우저의 `llm-chess-arena` IndexedDB에만 저장됩니다. 다른 기기와 자동 동기화되지 않으므로 필요하면 데이터 화면에서 JSON 백업을 내려받으세요.

## 로컬 실행

```bash
npm ci
npm run stockfish:copy
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
trunk serve --open
```

## 검증

```bash
cargo test
cargo check --target wasm32-unknown-unknown
trunk build --release --public-url /llm-chess-arena/
```

## 연구·라이선스

프롬프트 규약과 벤치마크 평점은 *LLM CHESS: Benchmarking Reasoning and Instruction-Following in LLMs through Chess* ([arXiv:2512.01992](https://arxiv.org/pdf/2512.01992))를 참고했습니다.

앱 코드는 `AGPL-3.0-or-later`입니다. 번들된 Stockfish.js/Stockfish 18은 GPLv3이며 원문은 `static/stockfish/COPYING`에 포함됩니다. 자세한 출처는 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)를 확인하세요.
