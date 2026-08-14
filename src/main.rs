#[cfg(target_arch = "wasm32")]
fn main() {
    yew::Renderer::<llm_chess_arena::app::App>::new().render();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    println!("Build this application for wasm32-unknown-unknown with Trunk.");
}
