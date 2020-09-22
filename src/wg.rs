use crate::{Component, Image, Pixel, PixelSize, SelfInserter};
use zerocopy::AsBytes;

type PixelBox<T> = euclid::Box2D<T, Pixel>;
pub trait WgpuUpload: Component {
    type Result: SelfInserter;
    fn upload(&self, wgpu: &mut Wgpu) -> Self::Result;
}

impl WgpuUpload for image::RgbaImage {
    type Result = TextureView;
    fn upload(&self, wgpu: &mut Wgpu) -> Self::Result {
        let material_id = {
            let (width, height) = self.dimensions();
            wgpu.create_material(PixelSize::new(width, height))
        };
        let material = &mut wgpu.materials[material_id.0];
        let rect = PixelBox::new(
            euclid::point2(0, 0),
            euclid::point2(material.size.width as u16, material.size.height as u16),
        );
        let view = TextureView { material_id, rect };
        wgpu.upload_texture(view, self);
        view
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TextureView {
    pub material_id: MaterialId,
    pub rect: PixelBox<u16>,
}
impl SelfInserter for TextureView {}

#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq)]
pub struct MaterialId(usize);

#[derive(Debug)]
pub struct Wgpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub texture_layout: wgpu::BindGroupLayout,
    pub uniform_layout: wgpu::BindGroupLayout,
    materials: slab::Slab<WgpuTexture>,
}
impl Wgpu {
    pub async fn init() -> Self {
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
        println!("{:?}\n", adapter.get_info());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();

        Wgpu::from_device_and_queue(device, queue)
    }
    fn from_device_and_queue(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        use std::convert::TryInto;
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("MaterialLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
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
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("UniformLayout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: (std::mem::size_of::<SpriteUniforms>() as u64)
                        .try_into()
                        .ok(),
                },
                count: None,
            }],
        });
        Self {
            device,
            queue,
            texture_layout,
            uniform_layout,
            materials: Default::default(),
        }
    }
    pub fn material(&self, id: MaterialId) -> &WgpuTexture {
        &self.materials[id.0]
    }
    pub fn create_material(&mut self, size: PixelSize<u32>) -> MaterialId {
        let size = wgpu::Extent3d {
            width: size.width,
            height: size.height,
            depth: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_DST,
        });

        let view = texture.create_view(&Default::default());
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_layout,
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

        let texture = WgpuTexture {
            texture,
            view,
            sampler,
            bind_group,
            size,
        };
        MaterialId(self.materials.insert(texture))
    }
    pub fn upload_texture(&self, view: TextureView, data: &[u8]) {
        let material = &self.materials[view.material_id.0];
        let width = view.rect.width() as u32;
        let height = view.rect.height() as u32;
        self.queue.write_texture(
            wgpu::TextureCopyView {
                texture: &material.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: view.rect.min.x as u32,
                    y: view.rect.min.y as u32,
                    z: 0,
                },
            },
            data,
            wgpu::TextureDataLayout {
                offset: 0,
                bytes_per_row: 4 * width,
                rows_per_image: height,
            },
            wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
        );
    }
}

#[repr(C)]
#[derive(AsBytes)]
pub struct SpriteUniforms {
    pub projection_matrix: [f32; 16],
}

#[derive(Debug)]
pub struct WgpuTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub bind_group: wgpu::BindGroup,
    pub size: wgpu::Extent3d,
}

pub struct SizedTexture {
    texture: wgpu::Texture,
    size: wgpu::Extent3d,
}

impl SizedTexture {
    pub fn new(device: &wgpu::Device, size: wgpu::Extent3d) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::COPY_SRC,
        });
        Self { texture, size }
    }
    pub fn view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
    pub fn save_to_buffer(&self, wgpu: &Wgpu) -> SizedBuffer {
        let sized_buffer = SizedBuffer::new(&wgpu.device, self.size);

        let command_buffer = {
            let mut encoder = wgpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

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

pub struct SizedBuffer {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
}

impl SizedBuffer {
    fn new(
        device: &wgpu::Device,
        wgpu::Extent3d {
            width,
            height,
            depth,
        }: wgpu::Extent3d,
    ) -> Self {
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
    pub async fn save_to_png(&self, device: &wgpu::Device, path: &str) {
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

            let file = std::fs::File::create(path).unwrap();
            let writer = std::io::BufWriter::new(file);

            let mut png_encoder = png::Encoder::new(writer, self.width, self.height);
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
