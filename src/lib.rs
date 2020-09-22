mod assets;
mod library;
mod sprite_map;
mod wg;

use assets::{AssetKey, Assets, IntoComponents, Load, SelfInserter};
use library::{Image, ImageOffset, ImageSize, Library};
use sprite_map::SpriteMap;
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
    pub async fn render_map(&mut self, map: &str, output: &str) {
        println!("Loading map...");
        let mut map = SpriteMap::open(map, &self.library, &mut self.assets);

        println!("Sorting map sprites...");
        map.sort_sprites();

        println!("Loading assets...");
        self.assets.load(&self.library);

        println!("Uploading textures to gpu...");
        //self.assets.wgpu_upload::<image::RgbaImage>(&mut self.wgpu);
        self.assets.sized_upload(&mut self.wgpu);

        println!("Rendering...");
        let sized_buffer = map.render(&self.wgpu, &self.assets);

        println!("Saving to png...");
        sized_buffer.save_to_png(&self.wgpu.device, output).await;

        println!("Done!");
    }
}
