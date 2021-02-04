mod assets;
mod library;
mod sprite_map;
mod wg;
mod config;

use assets::{AssetKey, Assets, IntoComponents, Load, SelfInserter};
use library::{Image, ImageOffset, ImageSize, Library};
use sprite_map::{SpriteMap, SpriteMapRenderer};
use wg::{MaterialId, SizedBuffer, SizedTexture, SpriteUniforms, TextureView, Wgpu, WgpuUpload};
use config::Config;

use hecs::Component;
pub struct Pixel;
pub type PixelSize<T> = euclid::Size2D<T, Pixel>;

pub struct State {
    library: Library,
    assets: Assets,
    wgpu: Wgpu,
    pub config: Config,
}
impl State {
    pub async fn new() -> Self {
        let config = Config::load();

        println!("Loading library...");
        let library = Library::load(&config.paths);
        let assets = Assets::new();

        println!("Initializing GPU...");
        let wgpu = Wgpu::init(config.window.low_power).await;

        println!("Ready to work!");

        Self {
            library,
            assets,
            wgpu,
            config,
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
        let renderer = map.into_renderer(&self.wgpu, &self.assets, format, &self.config);

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
        let mut width = self.config.window.width;
        let mut height =  self.config.window.height;

        println!("Creating window...");

        let event_loop = winit::event_loop::EventLoop::new();
        let window = winit::window::WindowBuilder::new()
            .with_inner_size(winit::dpi::PhysicalSize::new(width, height))
            .with_title("MapViewer")
            .build(&event_loop)
            .unwrap();


        println!("Creating surface...");

        let surface = unsafe { self.wgpu.instance.create_surface(&window) };

        println!("Creating swapchain...");

        let mut swapchain = self.wgpu.device.create_swap_chain(
            &surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
                format,
                width,
                height,
                present_mode: wgpu::PresentMode::Fifo,
            },
        );

        let begin = std::time::Instant::now();
        let mut max_zoom = renderer.max_zoom(width, height);
        let min_zoom = 10.0;
        let mut zoom = max_zoom;

        let mut shift_x = 0.0;
        let mut shift_y = 0.0;
        #[derive(Default)]
        struct Keys {
            left: bool,
            right: bool,
            up: bool,
            down: bool,
        }
        impl Keys {
            fn shift_x(&self) -> f32 {
                (if self.left {1.0} else {0.0}) +
                (if self.right {-1.0} else {0.0})
            }
            fn shift_y(&self) -> f32 {
                (if self.up {-1.0} else {0.0}) +
                (if self.down {1.0} else {0.0})
            }
            fn input(&mut self, key:  winit::event::VirtualKeyCode, state: winit::event::ElementState) {
                let pressed = state == winit::event::ElementState::Pressed;
                use winit::event::VirtualKeyCode::*;
                *match key {
                    Left => &mut self.left,
                    Up => &mut self.up,
                    Right => &mut self.right,
                    Down => &mut self.down,
                    _ => return,
                } = pressed;
            }
        }
        let mut keys = Keys::default();

        println!("Rendering...");

        event_loop.run(move |event, _event_loop, control_flow| {
            use winit::{
                event::{Event, MouseScrollDelta, StartCause, WindowEvent, KeyboardInput, VirtualKeyCode, ElementState},
                event_loop::ControlFlow,
            };
            match event {
                Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                    window.request_redraw();
                }
                Event::WindowEvent { window_id, event } => {
                    match event {
                        WindowEvent::Resized(new_size) => {
                            width = new_size.width;
                            height = new_size.height;
                            swapchain = self.wgpu.device.create_swap_chain(
                                &surface,
                                &wgpu::SwapChainDescriptor {
                                    usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
                                    format,
                                    width,
                                    height,
                                    present_mode: wgpu::PresentMode::Fifo,
                                },
                            );
                            max_zoom = renderer.max_zoom(width, height);
                            zoom = zoom.max(max_zoom).min(min_zoom);
                        }
                        WindowEvent::MouseWheel {
                            /*device_id: DeviceId,
                            phase: TouchPhase,
                            #[deprecated = "Deprecated in favor of WindowEvent::ModifiersChanged"]
                            modifiers: ModifiersState,*/
                            delta,
                            ..
                        } => {
                            let scroll = 1.0
                                + match delta {
                                    MouseScrollDelta::LineDelta(_, lines_y) => lines_y * 0.1,
                                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                                };
                            zoom = (zoom * scroll).max(max_zoom).min(min_zoom);
                        }
                        WindowEvent::KeyboardInput {
                            input: KeyboardInput{state, virtual_keycode: Some(key), ..},
                            ..
                        } => {
                            keys.input(key, state);
                        },
                        WindowEvent::CloseRequested => {
                            *control_flow = ControlFlow::Exit;
                        }
                        _ => {}
                    }
                }
                Event::RedrawRequested(_window_id) => {
                    //let time = begin.elapsed().as_secs_f32() * 0.1;
                    //shift_x = time.cos();
                    shift_x = (shift_x + keys.shift_x() * 0.002 / zoom).min(1.0).max(-1.0);
                    shift_y = (shift_y + keys.shift_y() * 0.002 / zoom).min(1.0).max(-1.0);
                    let frame = swapchain.get_current_frame().unwrap();
                    let view = &frame.output.view;
                    renderer.render_view(&self.wgpu, view, width, height, zoom, shift_x, shift_y);
                    *control_flow = ControlFlow::WaitUntil(
                        std::time::Instant::now() + std::time::Duration::from_millis(1000 / 60),
                    );
                }
                _ => {}
            }
        });
    }
}
