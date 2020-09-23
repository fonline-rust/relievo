mod assets;
mod library;
mod sprite_map;
mod wg;

use assets::{AssetKey, Assets, IntoComponents, Load, SelfInserter};
use library::{Image, ImageOffset, ImageSize, Library};
use sprite_map::{SpriteMap, SpriteMapRenderer};
use wg::{MaterialId, SizedBuffer, SizedTexture, SpriteUniforms, TextureView, Wgpu, WgpuUpload};

use hecs::Component;
pub struct Pixel;
pub type PixelSize<T> = euclid::Size2D<T, Pixel>;

pub struct State {
    library: Library,
    assets: Assets,
    wgpu: Wgpu,
}
impl State {
    pub async fn new() -> Self {
        println!("Loading library...");
        let library = Library::load();
        let assets = Assets::new();

        println!("Initializing GPU...");
        let wgpu = Wgpu::init().await;

        println!("Ready to work!");

        Self {
            library,
            assets,
            wgpu,
        }
    }
    fn prepare_map(&mut self, map: &str, format: wgpu::TextureFormat) -> SpriteMapRenderer {
        println!("Loading map...");
        let mut map = SpriteMap::open(map, &self.library, &mut self.assets);

        println!("Sorting map sprites...");
        map.sort_sprites();

        println!("Loading assets...");
        self.assets.load(&self.library);

        println!("Uploading textures to gpu...");
        //self.assets.wgpu_upload::<image::RgbaImage>(&mut self.wgpu);
        self.assets.sized_upload(&mut self.wgpu);

        println!("Prepare pipeline...");
        let renderer = map.into_renderer(&self.wgpu, &self.assets, format);

        renderer
    }
    pub async fn render_map(&mut self, map: &str, output: &str) {
        let renderer = self.prepare_map(map, wgpu::TextureFormat::Rgba8UnormSrgb);
        
        println!("Rendering...");
        let sized_buffer = renderer.render_into_texture(&self.wgpu);

        println!("Saving to png...");
        sized_buffer.save_to_png(&self.wgpu.device, output).await;

        println!("Done!");
    }
    pub fn show_map(mut self, map: &str) -> ! {
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let renderer = self.prepare_map(map, format);
        let mut width = 1000;
        let mut height = 300;

        let event_loop = winit::event_loop::EventLoop::new();
        let window = winit::window::WindowBuilder::new()
            .with_inner_size( winit::dpi::PhysicalSize::new(width, height))
            .with_title("MapViewer")
            .build(&event_loop)
            .unwrap();

        let surface = unsafe { self.wgpu.instance.create_surface(&window) };

        let mut swapchain = self.wgpu.device.create_swap_chain(&surface, &wgpu::SwapChainDescriptor{
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
        });

        let begin = std::time::Instant::now();
        let mut max_zoom = renderer.max_zoom(width, height);
        let min_zoom = 10.0;
        let mut zoom = max_zoom;

        event_loop.run(move |event, _event_loop, control_flow| {
            use winit::{event::{Event, WindowEvent, StartCause, MouseScrollDelta}, event_loop::ControlFlow};
            match event {
                Event::NewEvents(StartCause::ResumeTimeReached{..}) => {
                    window.request_redraw();
                }
                Event::WindowEvent {
                    window_id,
                    event
                } => {
                    match event {
                        WindowEvent::Resized(new_size) => {
                            width = new_size.width;
                            height = new_size.height;
                            swapchain = self.wgpu.device.create_swap_chain(&surface, &wgpu::SwapChainDescriptor{
                                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                                format,
                                width,
                                height,
                                present_mode: wgpu::PresentMode::Fifo,
                            });
                            max_zoom = renderer.max_zoom(width, height);
                            zoom = zoom.max(max_zoom).min(min_zoom);
                        },
                        WindowEvent::MouseWheel {
                            /*device_id: DeviceId,
                            phase: TouchPhase,
                            #[deprecated = "Deprecated in favor of WindowEvent::ModifiersChanged"]
                            modifiers: ModifiersState,*/
                            delta, ..
                        } => {
                            let scroll = 1.0 + match delta {
                                MouseScrollDelta::LineDelta(_, lines_y) => {
                                    lines_y*0.1
                                },
                                MouseScrollDelta::PixelDelta(pos) => {
                                    pos.y as f32 * 0.01
                                }
                            };
                            zoom = (zoom * scroll).max(max_zoom).min(min_zoom);
                        },
                        _ => {},
                    }
                },
                Event::RedrawRequested(_window_id) => {
                    let frame = swapchain.get_current_frame().unwrap();
                    let view = &frame.output.view;
                    renderer.render_view(&self.wgpu, view, width, height, zoom);
                    *control_flow = ControlFlow::WaitUntil(std::time::Instant::now() + std::time::Duration::from_millis(1000/30));
                },
                _ => {}
            }
        });
    }
}
