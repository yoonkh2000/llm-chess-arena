# Third-party notices

## Stockfish.js / Stockfish 18

- npm package: `stockfish@18.0.8`
- package integrity: `sha512-z+f2UMPXLylDBGjv9e9zU8QulY7hUl8MYHesLRrdddewlOXjJrUSmtNmbtID1/F72EPhq0CCkCNxgWS5MQVWtQ==`
- bundled build: `stockfish-18-lite-single.js` and `.wasm`
- source: <https://github.com/nmrugg/stockfish.js>
- upstream engine: <https://github.com/official-stockfish/Stockfish>
- license: GPLv3; full text at `static/stockfish/COPYING`

The lite single-thread build is used so the engine can run on GitHub Pages without cross-origin isolation headers. It is substantially stronger than its configured limited-strength opponent settings, while keeping the download around 7 MB.

## Rust and JavaScript dependencies

The application also uses Yew, shakmaty, rexie, wasm-bindgen, serde, uuid, gloo, and their transitive dependencies. Exact versions and checksums are pinned in `Cargo.lock` and `package-lock.json`; their respective license terms remain applicable.

## Research reference

Product behavior is informed by *LLM CHESS: Benchmarking Reasoning and Instruction-Following in LLMs through Chess*, arXiv:2512.01992: <https://arxiv.org/pdf/2512.01992>.
