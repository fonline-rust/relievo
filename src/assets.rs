use bumpalo::{collections::Vec as BumpVec, Bump};
use std::collections::{BTreeSet, HashMap};

use crate::{Component, Library, Pixel, TextureView, Wgpu, WgpuUpload};

#[derive(Debug, Copy, Clone)]
pub struct AssetKey(pub hecs::Entity);

pub struct Assets {
    //TODO: make private??
    pub world: hecs::World,
    from_path: HashMap<String, AssetKey>,
    bump: Bump,
}

#[derive(Debug)]
struct AssetPath(String);

enum AssetLoaderStatus {
    Loading,
    Unloaded,
    Error(String),
}

type InsertFn<'a> = &'a mut dyn FnMut(&mut hecs::World, hecs::Entity);
//type LoadFn = Box<dyn FnOnce(&Library, &str) -> Result<InsertFn, String>>;
type LoadFn = for<'a> fn(&Library, &str, &'a Bump) -> Result<InsertFn<'a>, String>;

fn load_fn<L: Load + IntoComponents>() -> LoadFn {
    |library, path, bump| {
        let mut res = Some(L::load(path, library)?);
        Ok(
            bump.alloc(move |world: &mut hecs::World, entity: hecs::Entity| {
                let inserter = res.take().expect("called twice");
                world.insert(entity, inserter.into_components()).unwrap();
            }),
        )
    }
}

struct AssetLoader {
    load: LoadFn,
    status: AssetLoaderStatus,
}

struct AssetStatistics {
    pub upserted: u32,
}

//struct AssetLoader;

impl Assets {
    pub fn new() -> Self {
        Self {
            world: hecs::World::new(),
            from_path: HashMap::new(),
            bump: Bump::with_capacity(100 * 1024),
        }
    }
    /*pub fn upsert_path<L: Load>(&mut self, path: &str) -> AssetKey {
        if let Some(key) = self.from_path.get(path) {
            return *key;
        }
        let key = AssetKey(self.world.spawn((AssetPath(path.to_owned()), AssetLoader)));
        self.from_path.insert(path.to_owned(), key);
        key
    }
    pub fn load(&mut self, library: &Library) {
        let loading: Vec<_> = self.world.query::<(&AssetPath, )>().with::<AssetLoader>().iter().flat_map( |(entity, (path, ))| {
            library.retriever.get_rgba(&path.0).ok().map(|raw| (entity, raw))
        }).collect();
        for (entity, image) in loading {
            self.world.insert_one(entity, image).unwrap();
            self.world.remove_one::<AssetLoader>(entity).unwrap();
        }
    }*/
    pub fn upsert_path<L: Load + IntoComponents>(&mut self, path: &str) -> AssetKey {
        if let Some(key) = self.from_path.get(path) {
            let mut usage = self.world.get_mut::<AssetStatistics>(key.0).unwrap();
            usage.upserted += 1;
            return *key;
        }
        let asset_loader = AssetLoader {
            load: load_fn::<L>(),
            status: AssetLoaderStatus::Loading,
        };
        let key = AssetKey(self.world.spawn((
            AssetPath(path.to_owned()),
            AssetStatistics { upserted: 1 },
            asset_loader,
        )));
        self.from_path.insert(path.to_owned(), key);
        key
    }
    pub fn load(&mut self, library: &Library) {
        let Self {
            world, ref bump, ..
        } = self;
        let loading = BumpVec::from_iter_in(
            world
                .query::<(&AssetPath, &mut AssetLoader)>()
                .iter()
                .flat_map(|(entity, (path, asset_loader))| {
                    //library.retriever.get_rgba(&path.0).ok().map(|raw| (entity, raw))
                    match (asset_loader.load)(library, &path.0, bump) {
                        Ok(inserter) => Some((entity, inserter)),
                        Err(err) => {
                            asset_loader.status = AssetLoaderStatus::Error(err);
                            None
                        }
                    }
                }),
            bump,
        );
        for (entity, inserter) in loading {
            (inserter)(world, entity);
            world.remove_one::<AssetLoader>(entity).unwrap();
        }
        self.bump.reset();
    }
    pub fn wgpu_upload<U: WgpuUpload>(&mut self, wgpu: &mut Wgpu) {
        let Self {
            world, ref bump, ..
        } = self;
        let loading = BumpVec::from_iter_in(
            world
                .query::<(&U,)>()
                .without::<U::Result>()
                .iter()
                .map(|(entity, (to_upload,))| (entity, to_upload.upload(wgpu))),
            bump,
        );
        for (entity, inserter) in loading {
            world.insert(entity, inserter.into_components()).unwrap();
        }
        self.bump.reset();
    }
    pub fn sized_upload(&mut self, wgpu: &mut Wgpu) {
        let Self {
            world, ref bump, ..
        } = self;

        const TEXTURE_MAX_SIZE: euclid::Size2D<u32, Pixel> = euclid::size2(4096, 4096);
        use std::cmp::Reverse;
        let mut total_size: euclid::Size2D<u32, Pixel> = euclid::size2(0, 0);
        let sorted: BTreeSet<_> = world
            .query::<(&crate::ImageSize, &AssetStatistics)>()
            .without::<crate::TextureView>()
            .iter()
            .map(|(entity, (size, usage))| {
                let size_u32 = size.0.to_u32();
                assert!(size_u32.greater_than(TEXTURE_MAX_SIZE).none());
                total_size += size_u32;
                (
                    Reverse(usage.upserted),
                    Reverse(size.0.height),
                    Reverse(size.0.width),
                    entity,
                )
            })
            .collect();

        if sorted.is_empty() {
            return;
        }
        let mut iter = sorted.into_iter().peekable();

        'new_atlas: loop {
            let atlas_width = total_size
                .width
                .min(TEXTURE_MAX_SIZE.width)
                .next_power_of_two();
            let atlas_height = total_size
                .height
                .min(TEXTURE_MAX_SIZE.height)
                .next_power_of_two();
            let atlas_size = euclid::size2(atlas_width, atlas_height);
            let mut atlas =
                guillotiere::SimpleAtlasAllocator::new(atlas_size.to_untyped().to_i32());
            let material_id = wgpu.create_material(atlas_size);

            'new_entity: loop {
                if let Some((_, Reverse(height), Reverse(width), entity)) = iter.peek() {
                    if let Some(rect) = atlas.allocate(euclid::size2(*width, *height).to_i32()) {
                        let view = TextureView {
                            material_id,
                            rect: rect.cast().cast_unit(),
                        };
                        {
                            let image = world.get::<image::RgbaImage>(*entity).unwrap();
                            wgpu.upload_texture(view, &image);
                        }
                        world.insert_one(*entity, view).unwrap();
                        let _ = iter.next();
                    } else {
                        break 'new_entity;
                    }
                } else {
                    break 'new_atlas;
                }
            }
        }

        //self.bump.reset();
    }
    pub fn _get<T: hecs::Component>(&self, key: AssetKey) -> Option<hecs::Ref<T>> {
        self.world.get(key.0).ok()
    }
    /*
    pub fn group_by_size(&self) {
        let Self {
            world, bump, ..
        } = self;

    }*/
}
/*
const fn log2(mut x: usize) -> usize {
    let mut res = 0;
    let x = x.next_power_of_two();
    while x > 0 {
        let x = x / 2;
        res += 1;
    }
    res
}

const fn sizes(mut from: usize, to: usize) -> &'static [usize] {
    let sizes = [0usize, log2(to-from)];
    for (i, val) in sizes.iter_mut() {
        *val =
    }
}
*/
pub trait Load: Sized + 'static {
    fn load(path: &str, library: &Library) -> Result<Self, String>;
}

pub trait IntoComponents {
    type Components: hecs::DynamicBundle;
    fn into_components(self) -> Self::Components;
}

pub trait SelfInserter: Component {}
impl<T: SelfInserter> IntoComponents for T {
    type Components = (T,);
    fn into_components(self) -> Self::Components {
        (self,)
    }
}
