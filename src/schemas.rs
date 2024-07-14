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
    pub steam_webapi_key: String,
}

#[derive(Deserialize)]
pub struct GetPublishedFileDetailsResponse {
    pub response: GetPublishedFileDetailsResponseInner,
}

#[derive(Deserialize)]
pub struct GetPublishedFileDetailsResponseInner {
    pub publishedfiledetails: Vec<GetPublishedFileDetailsResponseItem>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum GetPublishedFileDetailsResponseItem {
    FileDetails(PublishedFileDetails),
    MissingItem { result: i32, publishedfileid: String }
}

#[derive(Deserialize, Clone)]
pub struct PublishedFileDetails {
    pub publishedfileid: String,
    pub title: String,
    /// ctime
    pub time_updated: i64,
    pub children: Option<Vec<PublishedFileChild>>,
}

#[derive(Deserialize, Clone)]
pub struct PublishedFileChild {
    pub publishedfileid: String,
}
