use relievo::State;

async fn run() {
    let mut state = State::new().await;
    let map = std::env::args()
        .nth(1)
        .unwrap_or_else(|| state.config.open_map.clone());
    let output = format!("{}.png", &map);
    state.render_map(&map, &output).await;
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        //subscriber::initialize_default_subscriber(None);
        tracing_subscriber::fmt::init();
        futures::executor::block_on(run());
    }
    #[cfg(target_arch = "wasm32")]
    {
        /*std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run());*/
        unimplemented!()
    }
}
