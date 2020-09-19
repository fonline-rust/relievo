mod library;
use library::{Library, SpriteMap, Wgpu};

//mod png_saver;

/// This example shows how to describe the adapter in use.
async fn run(mut map: SpriteMap) {
    let adapter = wgpu::Instance::new(wgpu::BackendBit::PRIMARY)
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::Default,
            compatible_surface: None,
        })
        .await
        .unwrap();

    /*for adapter in wgpu::Instance::new(wgpu::BackendBit::PRIMARY).enumerate_adapters(wgpu::BackendBit::all()) {
        println!("{:?}\n", adapter.get_info());
        println!("{:?}\n\n", adapter.limits());
    }*/
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor{
        .. Default::default()
    }, None).await.unwrap();
    
    let wgpu = Wgpu::new(device, queue);

    println!("Uploading!");
    map.upload(&wgpu);
    println!("Rendering!");
    map.render(&wgpu, "foo.png").await;
    println!("Done!");
}

fn main() {
    let library = Library::load();
    let mut map = library::SpriteMap::open("../../fo/FO4RP/maps/fort_riverdale.fomap", &library);
    map.sort_sprites();
    map.load_data(&library);
    #[cfg(not(target_arch = "wasm32"))]
    {
        //subscriber::initialize_default_subscriber(None);
        tracing_subscriber::fmt::init();
        futures::executor::block_on(run(map));
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init().expect("could not initialize logger");
        wasm_bindgen_futures::spawn_local(run());
    }
}
