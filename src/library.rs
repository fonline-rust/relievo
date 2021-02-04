use crate::{IntoComponents, Load, Pixel, config};
use fo_data::{Converter, FoData, Retriever};
use std::collections::BTreeMap;

#[cfg(not(feature = "sled-retriever"))]
type MyRetriever = fo_data::FoRetriever;
#[cfg(feature = "sled-retriever")]
type MyRetriever = fo_data::SledRetriever;

pub struct Library {
    items: BTreeMap<u16, fo_proto_format::ProtoItem>,
    retriever: MyRetriever,
}

impl Library {
    pub fn load(paths: &config::Paths) -> Self {
        let items = fo_proto_format::build_btree(&paths.items_lst);

        let retriever;
        #[cfg(not(feature = "sled-retriever"))]
        {
            retriever = FoData::init(&paths.client, &paths.pallette)
            .expect("FoData loading")
            .into_retriever();
        }
        #[cfg(feature = "sled-retriever")]
        {
            retriever = MyRetriever::init("D:\\fo\\test_assets\\db\\assets", &paths.pallette).unwrap();
        }

        /*println!(
            "FoData loaded, archives: {}, files: {}",
            retriever.data().count_archives(),
            retriever.data().count_files()
        );*/
        println!(
            "FoData loaded"
        );

        Self { items, retriever }
    }
    pub fn with_proto<'a>(
        &'a self,
        obj: &'a fo_map_format::Object,
    ) -> Option<(&'a fo_map_format::Object, &'a fo_proto_format::ProtoItem)> {
        self.items.get(&obj.proto_id).map(|proto| (obj, proto))
    }
}

pub type Image = fo_data::RawImage;
impl Load for Image {
    fn load(path: &str, library: &Library) -> Result<Self, String> {
        library
            .retriever
            .get_rgba(&path)
            .map_err(|err| format!("{:?}", err))
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ImageSize(pub euclid::Size2D<u16, Pixel>);
#[derive(Debug, Default, Copy, Clone)]
pub struct ImageOffset {
    pub x: i16,
    pub y: i16,
}

impl IntoComponents for Image {
    type Components = (ImageSize, ImageOffset, image::RgbaImage);
    fn into_components(self) -> Self::Components {
        (
            ImageSize(euclid::size2(
                self.image.width() as u16,
                self.image.height() as u16,
            )),
            ImageOffset {
                x: self.offset_x,
                y: self.offset_y,
            },
            self.image,
        )
    }
}
