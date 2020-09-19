use std::collections::{BTreeMap, HashMap};
pub struct Library {
    items: BTreeMap<u16, fo_proto_format::ProtoItem>,
    data: fo_data::FoData,
}

impl Library {
    pub fn load() -> Self {
        let items = fo_proto_format::build_btree(
            "../../fo/FO4RP/proto/items/items.lst",
        );
        
        let data =
            fo_data::FoData::init("../../fo/CL4RP", "COLOR.PAL").expect("FoData loading");
        
        println!(
            "FoData loaded, archives: {}, files: {}",
            data.count_archives(),
            data.count_files()
        );

        Self {
            items, data,
        }
    }
}

#[derive(Debug)]
pub struct SpriteMap {
    rect: AABB,
    tiles: Vec<Sprite>,
    objects: Vec<Sprite>,
    assets: Assets<Image, WgpuTexture>,
}

type Image = fo_data::RawImage;

#[derive(Debug)]
struct Sprite {
    hex_x: u16,
    hex_y: u16,
    x: i32,
    y: i32,
    z: i32,
    asset: AssetKey,
}

#[derive(Copy, Clone, Debug)]
struct AssetKey(u16);

#[derive(Debug)]
struct Assets<T, U> {
    loaded: Vec<Option<T>>,
    uploaded: Vec<Option<U>>,
    to_path: Vec<String>,
    from_path: HashMap<String, AssetKey>,
}

impl<T, U> Assets<T, U> {
    fn new() -> Self {
        Self {
            loaded: vec![],
            uploaded: vec![],
            to_path: vec![],
            from_path: HashMap::new(),
        }
    }
    fn upsert_path(&mut self, path: &str) -> AssetKey {
        use std::convert::TryInto;
        if let Some(value) = self.from_path.get(path) {
            return *value;
        }
        let key = AssetKey(self.to_path.len().try_into().expect("65536 assets"));
        self.to_path.push(path.to_owned());
        self.from_path.insert(path.to_owned(), key);
        key
    }
}

trait Load: Sized {
    type Error: std::fmt::Debug;
    fn load(path: &str, library: &Library) -> Result<Self, Self::Error>;
}

trait Upload<G>: Sized {
    type Result;
    fn upload(&mut self, target: G) -> Self::Result;
}

impl<T: Load, U> Assets<T, U> where Option<T>: Clone {
    fn load_data(&mut self, library: &Library) {
        self.loaded.resize(self.to_path.len(), None);
        for (path, loaded) in self.to_path.iter().zip(&mut self.loaded) {
            if loaded.is_none() {
                println!("Loading \"{}\"", path);
                match T::load(path, library) {
                    Ok(data) => *loaded = Some(data),
                    Err(err) => {
                        println!("Can't load {}, because: {:?}", path, err);
                    }
                }
            }
        }
    }
}

/*impl<G, U, T: Upload<G, Result=U>> Assets<T, U> where Option<U>: Clone {
    fn upload(&self, target: G) -> T::Result {
        self.uploaded.resize(self.loaded.len(), None);
        for (loaded, uploaded) in self.loaded.iter().zip(&mut self.uploaded) {
            if uploaded.is_none() {
                if let Some(loaded) = loaded {
                    *uploaded = U::upload(loaded, target);
                }
            }
        }
    }
}*/

impl<G: Copy, U, T: Upload<G, Result=Option<U>>> Upload<G> for Assets<T, U> {
    type Result = ();
    fn upload(&mut self, target: G) -> Self::Result {
        self.uploaded.resize_with(self.loaded.len(), || None);
        for (loaded, uploaded) in self.loaded.iter_mut().zip(&mut self.uploaded) {
            if uploaded.is_none() {
                if let Some(loaded) = loaded {
                    *uploaded = T::upload(loaded, target);
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Wgpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub texture_layout: wgpu::BindGroupLayout,
    pub uniform_layout: wgpu::BindGroupLayout,
}
impl Wgpu {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        use std::convert::TryInto;
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{
            label: Some("MaterialLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry{
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        multisampled: false,
                        component_type: wgpu::TextureComponentType::Float,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler { comparison: false },
                    count: None,
                },
            ],
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{
            label: Some("UniformLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry{
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::UniformBuffer{
                        dynamic: false,
                        min_binding_size: (std::mem::size_of::<SpriteUniforms>() as u64).try_into().ok(),
                    },
                    count: None,
                },
            ],
        });
        Self {
            device, queue, texture_layout, uniform_layout
        }
    }
}

#[derive(Debug)]
pub struct WgpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
}

impl Upload<&Wgpu> for Image {
    type Result = Option<WgpuTexture>;
    fn upload(&mut self, target: &Wgpu) -> Option<WgpuTexture> {
        let dimensions = self.image.dimensions();
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth: 1,
        };
        let texture= target.device.create_texture(&wgpu::TextureDescriptor{
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
        });
        
        target.queue.write_texture(wgpu::TextureCopyView{
            texture: &texture,
            mip_level: 0,
            origin: Default::default()
        }, &self.image, wgpu::TextureDataLayout {
            offset: 0,
            bytes_per_row: 4 * dimensions.0,
            rows_per_image: dimensions.1,
        }, size);

        let view = texture.create_view(&Default::default());
        let sampler = target.device.create_sampler(
            &wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }
        );

        let bind_group = target.device.create_bind_group(&wgpu::BindGroupDescriptor{
            label: None,
            layout: &target.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Some(WgpuTexture{
            texture, view, sampler, bind_group
        })
    }
}

impl Load for Image {
    type Error = fo_data::GetImageError;
    fn load(path: &str, library: &Library) -> Result<Self, Self::Error> {
        library.data.get_rgba(&path)
    }
}

impl SpriteMap {
    pub fn open(path: &str, library: &Library) -> Self {
        use draw_geometry::fo as geometry;
        use primitives::Hex;
        use fo_map_format::Offset;

        fo_map_format::verbose_read_file(path, |_, res| {
            let map = res.unwrap().1;

            let mut assets = Assets::new();

            let tiles = map
                .tiles
                .0
                .iter()
                .filter(|tile| !tile.is_roof)
                .map(|tile| {
                    let (hex_x, hex_y) = (tile.hex_x, tile.hex_y);
                    let (offset_x, offset_y) = tile.offset();
                    let (x, y) = (hex_x as i32 , hex_y as i32);
                    let (x, y) = (
                        /*x = */ y * 16 - x * 24 - 24 + offset_x,
                        /*y = */ y * 12 + x * 6 + 24 + offset_y,
                    );
                    let z = geometry::draw_order_pos_int(
                        geometry::DRAW_ORDER_FLAT + tile.layer.unwrap_or(0) as u32,
                        Hex::new(tile.hex_x, tile.hex_y),
                    )
                    .unwrap_or(0);
                    
                    let asset = assets.upsert_path(map
                        .tiles
                        .1
                        .to_path
                        .get(&tile.hash)
                        .expect("Hash must have related conventional path"));

                    Sprite {
                        hex_x,
                        hex_y,
                        x,
                        y,
                        z,
                        asset
                    }
                })
                .collect();
            let objects = map
                .objects
                .0
                .iter()
                //.filter(|obj| obj.is_scenery())
                .filter(|obj| obj.kind.anim().is_some())
                .filter_map(|obj| library.items.get(&obj.proto_id).map(|proto| (obj, proto)))
                .filter(|(_obj, proto)| {
                    (proto.Flags.unwrap_or(0) & fo_defines_fo4rp::fos::ITEM_HIDDEN) == 0
                })
                .map(|(obj, proto)| {
                    let (hex_x, hex_y) = (obj.map_x.unwrap_or(0), obj.map_y.unwrap_or(0));
                    let (offset_x, offset_y) = obj.offset();
                    let (x, y) = (hex_x as i32, hex_y as i32);
                    let (x, y) = (
                        /*x = */ y * 16 - x * 24 - (x % 2) * 8 + offset_x,
                        /*y = */ y * 12 + x * 6 - (x % 2) * 6 + offset_y,
                    );
                    let z = geometry::draw_order_pos_int(
                        geometry::DrawOrderType::DRAW_ORDER_SCENERY as u32,
                        Hex::new(hex_x, hex_y),
                    ).unwrap_or(0);

                    let asset = assets.upsert_path(
                        &nom_prelude::make_path_conventional(&proto.PicMap)
                    );

                    Sprite {
                        hex_x,
                        hex_y,
                        x,
                        y,
                        z,
                        asset,
                    }
                })
                .collect();
            let rect = AABB::new();
            SpriteMap {
                rect,
                tiles,
                objects,
                assets,
            }
        }).unwrap()
    }
    pub fn sort_sprites(&mut self) {
        self.tiles.sort_by_key(|sprite| sprite.z);
        self.objects.sort_by_key(|sprite| sprite.z);
    }
    pub fn load_data(&mut self, library: &Library) {
        self.assets.load_data(library);
    }
    pub fn upload(&mut self, target: &Wgpu) {
        self.assets.upload(target);
    }
    fn calc_drawlist(&mut self) -> (Vec<SpriteVertex>, Vec<AssetKey>) {
        let mut vertices = vec![];
        let mut materials = vec![];
        for sprite in self.tiles.iter() {
            let image = match self.assets.loaded.get(sprite.asset.0 as usize) {
                Some(Some(image)) => image,
                _ => {
                    println!("None! {:?}", self.assets.to_path.get(sprite.asset.0 as usize));
                    continue
                }
            };
            let width = image.image.width() as i32;
            let height = image.image.height() as i32;
            let (offset_x, offset_y) = image.offsets();
            let x0 = sprite.x + offset_x as i32;
            let y0 = sprite.y + offset_y as i32;
            let x1 = x0 + width;
            let y1 = y0 + height;
            self.rect.insert_rect(x0, y0, x1, y1);
            //vertices.push(SpriteVertex{pos: [sprite.x as f32, sprite.y as f32], size: [width as f32, height as f32]});
            vertices.push(SpriteVertex{pos: [x0 as f32, y0 as f32], size: [width as f32, height as f32]});
            materials.push(sprite.asset);
        }
        for sprite in self.objects.iter() {
            let image = match self.assets.loaded.get(sprite.asset.0 as usize) {
                Some(Some(image)) => image,
                _ => {
                    println!("None! {:?}", self.assets.to_path.get(sprite.asset.0 as usize));
                    continue
                }
            };
            let width = image.image.width() as i32;
            let height = image.image.height() as i32;
            let (offset_x, offset_y) = image.offsets();
            let x0 = sprite.x + offset_x as i32;
            let y0 = sprite.y + offset_y as i32;
            let x1 = x0 + width;
            let y1 = y0 + height;
            self.rect.insert_rect(x0, y0, x1, y1);
            //vertices.push(SpriteVertex{pos: [sprite.x as f32, sprite.y as f32], size: [width as f32, height as f32]});
            vertices.push(SpriteVertex{pos: [x0 as f32, y0 as f32], size: [width as f32, height as f32]});
            materials.push(sprite.asset);
        }
        (vertices, materials)
    }
    pub async fn render(&mut self, wgpu: &Wgpu, path: &str) {
        let (vertices, materials) = self.calc_drawlist();
        use wgpu::util::DeviceExt;
        let vertex_buffer = wgpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: vertices.as_bytes(),
                usage: wgpu::BufferUsage::VERTEX,
            }
        );

        let dimensions = (self.rect.width().unwrap(), self.rect.height().unwrap());
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth: 1,
        };

        let uniforms = SpriteUniforms{
            screen_size: [size.width as f32, size.height as f32],
            screen_shift: [self.rect.top_left.0 as f32, self.rect.top_left.1 as f32],
        };
        let uniform_buffer = wgpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Uniforms"),
                contents: uniforms.as_bytes(),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            }
        );
        let uniform_bind_group = wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor{
            label: None,
            layout: &wgpu.uniform_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer{buffer: &uniform_buffer, offset: 0, size: None},
                },
            ],
        });

        let sized_texture = SizedTexture::new(&wgpu.device, size);

        let pipeline = sprite_pipeline(&wgpu, wgpu::TextureFormat::Rgba8UnormSrgb);

        let mut encoder =
            wgpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let view = sized_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
            rpass.set_pipeline(&pipeline);
            rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
            rpass.set_bind_group(0, &uniform_bind_group, &[]);
            for (material, i) in materials.into_iter().zip(0u32 ..) {
                let texture = self.assets.uploaded[material.0 as usize].as_ref().unwrap();
                rpass.set_bind_group(1, &texture.bind_group, &[]);
                rpass.draw(0..6, i..i+1);
            }
        }
    
        wgpu.queue.submit(Some(encoder.finish()));

        let buffer = sized_texture.save_to_buffer(wgpu);
        buffer.save_to_png(&wgpu.device, path).await;
    }
}

fn sprite_pipeline(wgpu: &Wgpu, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    // Load the shaders from disk
    let vs_module = wgpu.device.create_shader_module(wgpu::include_spirv!("shader.vert.spv"));
    let fs_module = wgpu.device.create_shader_module(wgpu::include_spirv!("shader.frag.spv"));

    let pipeline_layout = wgpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&wgpu.uniform_layout, &wgpu.texture_layout],
        push_constant_ranges: &[],
    });

    /*let color_states = &[wgpu::ColorStateDescriptor {
        format: format,
        color_blend: wgpu::BlendDescriptor {
            operation: wgpu::BlendOperation::Add,
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        },
        alpha_blend: wgpu::BlendDescriptor::REPLACE,
        write_mask: wgpu::ColorWrite::ALL,
    }];*/

    let color_states = &[wgpu::ColorStateDescriptor {
        format: format,
        color_blend: wgpu::BlendDescriptor {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha_blend: wgpu::BlendDescriptor {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        },
        write_mask: wgpu::ColorWrite::ALL,
    }];

    let render_pipeline = wgpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vs_module,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &fs_module,
            entry_point: "main",
        }),
        // Use the default rasterizer state: no culling, no depth bias
        rasterization_state: None,
        primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        color_states,
        depth_stencil_state: None,
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[SpriteVertex::desc()],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    render_pipeline
}

use zerocopy::AsBytes;
#[repr(C)]
#[derive(AsBytes)]
struct SpriteVertex {
    pos: [f32; 2],
    size: [f32; 2],
}

#[repr(C)]
#[derive(AsBytes)]
struct SpriteUniforms {
    screen_size: [f32; 2],
    screen_shift: [f32; 2],
}

impl SpriteVertex {
    fn desc<'a>() -> wgpu::VertexBufferDescriptor<'a> {
        wgpu::VertexBufferDescriptor {
            stride: std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Instance,
            attributes: &[
                wgpu::VertexAttributeDescriptor {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float2,
                },
                wgpu::VertexAttributeDescriptor {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float2,
                },
            ]
        }
    }
}
struct SizedTexture {
    texture: wgpu::Texture,
    size: wgpu::Extent3d,
}
struct SizedBuffer {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
}
impl SizedBuffer {
    fn new(device: &wgpu::Device, wgpu::Extent3d{width, height, depth}: wgpu::Extent3d) -> Self {
        assert_eq!(depth, 1);
        let bytes_per_pixel = std::mem::size_of::<u32>() as u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (padded_bytes_per_row * height) as u64,
            usage: wgpu::BufferUsage::MAP_READ | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            buffer,
        }
    }
    async fn save_to_png(&self, device: &wgpu::Device, path: &str) {
        use std::io::Write;

        // Note that we're not calling `.await` here.
        let buffer_slice = self.buffer.slice(..);
        let buffer_future = buffer_slice.map_async(wgpu::MapMode::Read);
        
        // Poll the device in a blocking manner so that our future resolves.
        // In an actual application, `device.poll(...)` should
        // be called in an event loop or on another thread.
        device.poll(wgpu::Maintain::Wait);
        // If a file system is available, write the buffer as a PNG
        let has_file_system_available = cfg!(not(target_arch = "wasm32"));
        if !has_file_system_available {
            return;
        }

        if let Ok(()) = buffer_future.await {
            let padded_buffer = buffer_slice.get_mapped_range();
            
            let mut png_encoder = png::Encoder::new(
                std::fs::File::create(path).unwrap(),
                self.width,
                self.height,
            );
            png_encoder.set_depth(png::BitDepth::Eight);
            png_encoder.set_color(png::ColorType::RGBA);
            let mut png_writer = png_encoder
                .write_header()
                .unwrap()
                .into_stream_writer_with_size(self.unpadded_bytes_per_row as usize);

            // from the padded_buffer we write just the unpadded bytes into the image
            for chunk in padded_buffer.chunks(self.padded_bytes_per_row as usize) {
                png_writer
                    .write(&chunk[..self.unpadded_bytes_per_row as usize])
                    .unwrap();
            }
            png_writer.finish().unwrap();

            // With the current interface, we have to make sure all mapped views are
            // dropped before we unmap the buffer.
            drop(padded_buffer);

            self.buffer.unmap();
        }
    }
}

impl SizedTexture {
    fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let texture= device.create_texture(&wgpu::TextureDescriptor{
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::COPY_SRC,
        });
        Self{texture, size}
    }
    fn save_to_buffer(&self, wgpu: &Wgpu) -> SizedBuffer {
        let sized_buffer = SizedBuffer::new(&wgpu.device, self.size);
        
        let command_buffer = {
            let mut encoder =
                wgpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            // Copy the data from the texture to the buffer
            encoder.copy_texture_to_buffer(
                wgpu::TextureCopyView {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                },
                wgpu::BufferCopyView {
                    buffer: &sized_buffer.buffer,
                    layout: wgpu::TextureDataLayout {
                        offset: 0,
                        bytes_per_row: sized_buffer.padded_bytes_per_row,
                        rows_per_image: 0,
                    },
                },
                self.size,
            );

            encoder.finish()
        };

        wgpu.queue.submit(Some(command_buffer));
        sized_buffer
    }
}



#[derive(Debug)]
struct AABB {
    top_left: (i32, i32),
    bottom_right: (i32, i32),
}
impl AABB {
    fn new() -> Self {
        Self {
            top_left: (i32::max_value(), i32::max_value()),
            bottom_right: (i32::min_value(), i32::min_value()),
        }
    }
    fn width(&self) -> Option<u32> {
        use std::convert::TryInto;
        self.bottom_right.0.checked_sub(self.top_left.0)?.try_into().ok()
    }
    fn height(&self) -> Option<u32> {
        use std::convert::TryInto;
        self.bottom_right.1.checked_sub(self.top_left.1)?.try_into().ok()
    }
    fn _insert(&mut self, x: i32, y: i32) {        
        if x < self.top_left.0 {
            self.top_left.0 = x;
        }
        if y < self.top_left.1 {
            self.top_left.1 = y;
        }
        if x > self.bottom_right.0 {
            self.bottom_right.0 = x;
        }
        if y > self.bottom_right.1 {
            self.bottom_right.1 = y;
        }
    }
    fn insert_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {        
        if x0 < self.top_left.0 {
            self.top_left.0 = x0;
        }
        if y0 < self.top_left.1 {
            self.top_left.1 = y0;
        }
        if x1 > self.bottom_right.0 {
            self.bottom_right.0 = x1;
        }
        if y1 > self.bottom_right.1 {
            self.bottom_right.1 = y1;
        }
    }
}
