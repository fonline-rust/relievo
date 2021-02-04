use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub open_map: String,
    pub window: Window,
    pub paths: Paths
}

impl Config {
    pub fn load() -> Self {
        let path = std::path::Path::new("config.toml");
        let config;
        if path.exists() {
            let string = std::fs::read_to_string(path).expect("Read config.toml");
            config = toml::from_str(&string).expect("Parse config.toml");
        } else {
            config = Default::default();
            let string = toml::to_string(&config).expect("Encode default config");
            std::fs::write(path, &string).expect("Write default convig.toml");
            panic!("Edit config.toml!");
        }
        config
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Window {
    pub width: u32,
    pub height: u32,
    pub background: [f64; 4]
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Paths {
    //pub maps: String,
    pub client: String,
    pub items_lst: String,
    pub pallette: String,
    pub shaders: String,
}
