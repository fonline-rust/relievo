use relievo::State;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tracing_subscriber::fmt::init();
        let state = futures::executor::block_on(State::new());
        let map = std::env::args()
            .nth(1)
            .unwrap_or_else(|| state.config.open_map.clone());
        state.show_map(&map);
    }
    #[cfg(target_arch = "wasm32")]
    {
        unimplemented!();
    }
}
