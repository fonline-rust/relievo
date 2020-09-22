use relievo::State;

async fn run() {
    let mut state = State::new().await;
    state
        .render_map(
            "../../fo/FO4RP/maps/fort_riverdale.fomap",
            "fort_riverdale.fomap",
        )
        .await;
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
