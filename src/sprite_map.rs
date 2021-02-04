use crate::{
    AssetKey, Assets, Image, ImageOffset, ImageSize, Library, MaterialId, SizedBuffer,
    SizedTexture, SpriteUniforms, TextureView, Wgpu, Config
};
use zerocopy::AsBytes;
use std::path::Path;

#[derive(Debug)]
pub struct SpriteMap {
    rect: AABB,
    tiles: Vec<Sprite>,
    objects: Vec<Sprite>,
    //assets: Assets<Image, WgpuTexture>,
}

#[derive(Debug)]
struct Sprite {
    hex_x: u16,
    hex_y: u16,
    x: i32,
    y: i32,
    z: i32,
    asset: AssetKey,
}

impl SpriteMap {
    pub fn open(path: &str, library: &Library, assets: &mut Assets) -> Self {
        use draw_geometry::fo as geometry;
        use fo_map_format::Offset;
        use primitives::Hex;

        fo_map_format::verbose_read_file(path, |_, res| {
            let map = res.unwrap().1;

            let tiles = map
                .tiles
                .0
                .iter()
                .filter(|tile| !tile.is_roof)
                .map(|tile| {
                    let (hex_x, hex_y) = (tile.hex_x, tile.hex_y);
                    let (offset_x, offset_y) = tile.offset();
                    let (x, y) = (hex_x as i32, hex_y as i32);
                    let (x, y) = (
                        /*x = */ y * 16 - x * 24 - 24 + offset_x,
                        /*y = */ y * 12 + x * 6 + 24 + offset_y,
                    );
                    let z = geometry::draw_order_pos_int(
                        geometry::DRAW_ORDER_FLAT + tile.layer.unwrap_or(0) as u32,
                        Hex::new(tile.hex_x, tile.hex_y),
                    )
                    .unwrap_or(0);

                    let asset = assets.upsert_path::<Image>(
                        map.tiles
                            .1
                            .to_path
                            .get(&tile.hash)
                            .expect("Hash must have related conventional path"),
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
            let objects = map
                .objects
                .0
                .iter()
                //.filter(|obj| obj.is_scenery())
                .filter(|obj| obj.kind.anim().is_some())
                .filter_map(|obj| library.with_proto(obj))
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

                    // TODO: handle flat items and scenery
                    let z = geometry::draw_order_pos_int(
                        geometry::DrawOrderType::DRAW_ORDER_SCENERY as u32,
                        Hex::new(hex_x, hex_y), //TODO: add + proto.DrawOrderOffsetHexY
                    )
                    .unwrap_or(0);

                    let asset = assets
                        .upsert_path::<Image>(&nom_prelude::make_path_conventional(&proto.PicMap));

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
            }
        })
        .unwrap()
    }
    pub fn sort_sprites(&mut self) {
        self.tiles.sort_by_key(|sprite| sprite.z);
        self.objects.sort_by_key(|sprite| sprite.z);
    }
    fn calc_drawlist(
        &mut self,
        assets: &Assets,
    ) -> (Vec<SpriteVertex>, Vec<(MaterialId, std::ops::Range<u32>)>) {
        let mut vertices = vec![];
        let mut materials: Vec<(MaterialId, std::ops::Range<u32>)> = vec![];
        let mut i = 0u32;
        for sprite in self.tiles.iter().chain(&self.objects) {
            if let Some((vertex, material_id)) = calc_sprite(assets, sprite, &mut self.rect) {
                match materials.last_mut() {
                    Some((last, range)) if *last == material_id => {
                        range.end += 1;
                    }
                    _ => {
                        materials.push((material_id, i..i + 1));
                    }
                }
                vertices.push(vertex);
                i += 1;
            }
        }
        /*let mut buf = std::collections::BTreeMap::new();
        for sprite in self.tiles.iter().chain(&self.objects) {
            if let Some((vertex, material_id)) = calc_sprite(assets, sprite, &mut self.rect) {
                buf.insert((sprite.z, material_id), vertex);
            }
        }
        let mut vertices = vec![];
        let mut materials: Vec<(MaterialId, std::ops::Range<u32>)> = vec![];
        let mut i = 0u32;
        for ((_, material_id), vertex) in buf {
            match materials.last_mut() {
                Some((last, range)) if *last == material_id => {
                    range.end += 1;
                },
                _ => {
                    materials.push((material_id, i..i+1));
                }
            }
            vertices.push(vertex);
            i += 1;
        }
        */
        (vertices, materials)
    }
    pub fn into_renderer(
        self,
        wgpu: &Wgpu,
        assets: &Assets,
        format: wgpu::TextureFormat,
        config: &Config,
    ) -> SpriteMapRenderer {
        SpriteMapRenderer::new(self, wgpu, assets, format, config)
    }
}

fn calc_sprite(
    assets: &Assets,
    sprite: &Sprite,
    rect: &mut AABB,
) -> Option<(SpriteVertex, MaterialId)> {
    let mut query = assets
        .world
        .query_one::<(&TextureView, &ImageSize, Option<&ImageOffset>)>(sprite.asset.0)
        .ok()?;
    let (view, size, offsets) = query.get()?;
    let width = size.0.width as i32;
    let height = size.0.height as i32;
    let offsets = offsets.copied().unwrap_or_default();
    let x0 = sprite.x + offsets.x as i32;
    let y0 = sprite.y + offsets.y as i32;
    let x1 = x0 + width;
    let y1 = y0 + height;
    rect.insert_rect(x0, y0, x1, y1);

    Some((
        SpriteVertex {
            pos: [x0 as f32, y0 as f32],
            size: [width as f32, height as f32],
            tex: [
                view.rect.min.x,
                view.rect.min.y,
                view.rect.max.x,
                view.rect.max.y,
            ],
        },
        view.material_id,
    ))
}

fn shader_module_from_file(device: &wgpu::Device, path: &Path) -> wgpu::ShaderModule {
    let file = std::fs::read(path).unwrap();
    let source = wgpu::util::make_spirv(
        &file);
    device.create_shader_module(&wgpu::ShaderModuleDescriptor{source, label: None, flags: Default::default()})
}

fn sprite_pipeline(wgpu: &Wgpu, format: wgpu::TextureFormat, shaders: &Path) -> wgpu::RenderPipeline {
    // Load the shaders from disk
    let vs_module = shader_module_from_file(&wgpu.device, &shaders.join("shader.vert.spv"));
    let fs_module = shader_module_from_file(&wgpu.device, &shaders.join("shader.frag.spv"));

    let pipeline_layout = wgpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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

    let color_states = &[wgpu::ColorTargetState {
        format: format,
        color_blend: wgpu::BlendState {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha_blend: wgpu::BlendState {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Max,
        },
        write_mask: wgpu::ColorWrite::ALL,
    }];

    let render_pipeline = wgpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: "main",
                buffers: &[SpriteVertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: "main",
                targets: color_states,
            }),
            // Use the default rasterizer state: no culling, no depth bias
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: wgpu::CullMode::None,
                polygon_mode: wgpu::PolygonMode::Fill,
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
        });

    render_pipeline
}

#[repr(C)]
#[derive(AsBytes)]
struct SpriteVertex {
    pos: [f32; 2],
    size: [f32; 2],
    tex: [u16; 4],
}

impl SpriteVertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::InputStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Ushort4,
                },
            ],
        }
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
        self.bottom_right
            .0
            .checked_sub(self.top_left.0)?
            .try_into()
            .ok()
    }
    fn height(&self) -> Option<u32> {
        use std::convert::TryInto;
        self.bottom_right
            .1
            .checked_sub(self.top_left.1)?
            .try_into()
            .ok()
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

pub struct SpriteMapRenderer {
    map: SpriteMap,
    drawlist: Vec<(MaterialId, std::ops::Range<u32>)>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    background: wgpu::Color,
}

impl SpriteMapRenderer {
    fn new(mut map: SpriteMap, wgpu: &Wgpu, assets: &Assets, format: wgpu::TextureFormat, config: &Config) -> Self {
        let (vertices, materials) = map.calc_drawlist(assets);
        use wgpu::util::DeviceExt;
        let vertex_buffer = wgpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: vertices.as_bytes(),
                usage: wgpu::BufferUsage::VERTEX,
            });

        /*
        let uniform_buffer = wgpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Uniforms"),
                contents: uniforms.as_bytes(),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            });
        */
        let uniform_buffer = wgpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniforms"),
            size: std::mem::size_of::<SpriteUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_bind_group = wgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &wgpu.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: None,
                },
            }],
        });

        let background = {
            let [r, g, b, a] = config.window.background;
            wgpu::Color{r, g, b, a}
        };

        let pipeline = sprite_pipeline(&wgpu, format, config.paths.shaders.as_ref());
        Self {
            map,
            drawlist: materials,
            pipeline,
            vertex_buffer,
            uniform_buffer,
            uniform_bind_group,
            background
        }
    }
    pub fn render_into_texture(&self, wgpu: &Wgpu) -> SizedBuffer {
        let rect = &self.map.rect;
        let dimensions = (rect.width().unwrap(), rect.height().unwrap());
        //let dimensions = (1920, 1080);

        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth: 1,
        };

        let matrix = {
            /*Transform2D::identity()
            .then_translate(vec2(-self.rect.top_left.0 as f32, -self.rect.top_left.1 as f32))
            .then_scale(1.0 / size.width as f32, 1.0 / size.height as f32)
            .to_3d()*/
            euclid::default::Transform3D::ortho(
                rect.top_left.0 as f32,
                rect.bottom_right.0 as f32,
                rect.bottom_right.1 as f32,
                rect.top_left.1 as f32,
                -1.0,
                1.0,
            )
        };

        let uniforms = SpriteUniforms {
            projection_matrix: matrix.to_array(),
        };

        let sized_texture = SizedTexture::new(&wgpu.device, size);
        self.render(wgpu, &sized_texture.view(), uniforms);
        sized_texture.save_to_buffer(wgpu)
    }

    fn xy_ratios(&self, width: u32, height: u32) -> (f32, f32) {
        let rect = &self.map.rect;
        let map_width = rect.width().unwrap() as f32;
        let map_height = rect.height().unwrap() as f32;
        let window_width = width as f32;
        let window_height = height as f32;

        let x_ratio = map_width / window_width;
        let y_ratio = map_height / window_height;

        (x_ratio, y_ratio)
    }

    pub fn max_zoom(&self, width: u32, height: u32) -> f32 {
        let (x_ratio, y_ratio) = self.xy_ratios(width, height);
        1.0 / x_ratio.max(y_ratio)
    }

    pub fn render_view(
        &self,
        wgpu: &Wgpu,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        zoom: f32,
        shift_x: f32,
        shift_y: f32,
    ) {
        let (x_ratio, y_ratio) = self.xy_ratios(width, height);
        let matrix = {
            let rect = &self.map.rect;
            euclid::default::Transform3D::ortho(
                rect.top_left.0 as f32,
                rect.bottom_right.0 as f32,
                rect.bottom_right.1 as f32,
                rect.top_left.1 as f32,
                -1.0,
                1.0,
            )
            .then_translate(euclid::vec3(shift_x, shift_y, 0.0))
            .then_scale(x_ratio * zoom, y_ratio * zoom, 1.0)
        };
        let uniforms = SpriteUniforms {
            projection_matrix: matrix.to_array(),
        };
        self.render(wgpu, view, uniforms);
    }
    fn render(&self, wgpu: &Wgpu, view: &wgpu::TextureView, uniforms: SpriteUniforms) {
        //dbg!(self.drawlist.len());
        //let before = std::time::Instant::now();

        wgpu.queue
            .write_buffer(&self.uniform_buffer, 0, uniforms.as_bytes());

        let mut encoder = wgpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.background),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
            rpass.set_pipeline(&self.pipeline);
            rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);

            //for (key, group) in &materials.into_iter().zip(0u32..).group_by(|(material_id, _)| material_id) {
            for (material_id, range) in self.drawlist.iter() {
                let texture = wgpu.material(*material_id);
                rpass.set_bind_group(1, &texture.bind_group, &[]);
                rpass.draw(0..6, range.clone());
            }
        }

        let command_buffer = Some(encoder.finish());
        wgpu.queue.submit(command_buffer);
        //wgpu.device.poll(wgpu::Maintain::Wait);

        //println!("Render completed in {} us", before.elapsed().as_micros());
    }
}
