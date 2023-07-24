use jomini::JominiDeserialize;
use serde::{Serialize, Deserialize};

#[derive(Deserialize, Serialize)]
pub struct Manifest {
    pub mods: Vec<Mod>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Mod {
    pub id: String,
    pub name: Option<String>,
    pub checksum: Option<String>,
}

/// Schema of descriptor.mod file
#[derive(JominiDeserialize)]
pub struct Descriptor {
    pub name: String,
    pub dependencies: Option<Vec<String>>,
    pub remote_file_id: Option<String>,
    pub supported_version: Option<String>,
    pub tags: Option<Vec<String>>,
    pub version: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub collection_path: String,
}
