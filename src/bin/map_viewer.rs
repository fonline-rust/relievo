use relievo::State;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tracing_subscriber::fmt::init();
        let state = futures::executor::block_on(State::new());
        state.show_map("../../fo/FO4RP/maps/fort_riverdale.fomap")
    }
    #[cfg(target_arch = "wasm32")]
    {
        unimplemented!()
    }
}
