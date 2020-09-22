use crate::{
    AssetKey, Assets, Image, ImageOffset, ImageSize, Library, MaterialId, SizedBuffer,
    SizedTexture, SpriteUniforms, TextureView, Wgpu,
};
use zerocopy::AsBytes;

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
    pub fn render(&mut self, wgpu: &Wgpu, assets: &Assets) -> SizedBuffer {
        let (vertices, materials) = self.calc_drawlist(assets);
        use wgpu::util::DeviceExt;
        let vertex_buffer = wgpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: vertices.as_bytes(),
                usage: wgpu::BufferUsage::VERTEX,
            });

        let dimensions = (self.rect.width().unwrap(), self.rect.height().unwrap());
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
                self.rect.top_left.0 as f32,
                self.rect.bottom_right.0 as f32,
                self.rect.bottom_right.1 as f32,
                self.rect.top_left.1 as f32,
                -1.0,
                1.0,
            )
        };

        let uniforms = SpriteUniforms {
            projection_matrix: matrix.to_array(),
        };
        let uniform_buffer = wgpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Uniforms"),
                contents: uniforms.as_bytes(),
                usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
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

        let sized_texture = SizedTexture::new(&wgpu.device, size);

        let pipeline = sprite_pipeline(&wgpu, wgpu::TextureFormat::Rgba8UnormSrgb);

        //dbg!(&materials);
        dbg!(materials.len());
        dbg!(vertices.len());

        let times = 100;
        let before_all = std::time::Instant::now();
        for _ in 0..times {
            let before = std::time::Instant::now();
            let mut encoder = wgpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let view = sized_texture.view();
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

                //for (key, group) in &materials.into_iter().zip(0u32..).group_by(|(material_id, _)| material_id) {
                for (material_id, range) in materials.iter() {
                    let texture = wgpu.material(*material_id);
                    rpass.set_bind_group(1, &texture.bind_group, &[]);
                    rpass.draw(0..6, range.clone());
                }
            }

            let command_buffer = Some(encoder.finish());
            wgpu.queue.submit(command_buffer);
            //wgpu.device.poll(wgpu::Maintain::Wait);

            println!("Render completed in {} us", before.elapsed().as_micros());
        }
        let all = before_all.elapsed().as_micros();
        println!(
            "All renders completed in {} us; {} us per render",
            all,
            all / times
        );

        sized_texture.save_to_buffer(wgpu)
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

fn sprite_pipeline(wgpu: &Wgpu, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    // Load the shaders from disk
    let vs_module = wgpu
        .device
        //.create_shader_module(wgpu::include_spirv!("shader.vert.spv"));
        .create_shader_module(wgpu::util::make_spirv(
            &std::fs::read("src/shader.vert.spv").unwrap(),
        ));
    let fs_module = wgpu
        .device
        //.create_shader_module(wgpu::include_spirv!("shader.frag.spv"));
        .create_shader_module(wgpu::util::make_spirv(
            &std::fs::read("src/shader.frag.spv").unwrap(),
        ));

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

    let render_pipeline = wgpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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

#[repr(C)]
#[derive(AsBytes)]
struct SpriteVertex {
    pos: [f32; 2],
    size: [f32; 2],
    tex: [u16; 4],
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
                wgpu::VertexAttributeDescriptor {
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
