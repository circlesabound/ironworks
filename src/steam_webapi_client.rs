use std::collections::HashMap;

use log::trace;
use reqwest::Method;

use crate::{error::Result, schemas::{GetPublishedFileDetailsResponse, GetPublishedFileDetailsResponseItem, PublishedFileDetails}};

pub struct SteamWebApiClient {
    client: reqwest::Client,
    webapi_key: String,
}

const STELLARIS_APPID: &str = "281990";
const STEAM_WEBAPI_GETDETAILS_URL: &str = "https://api.steampowered.com/IPublishedFileService/GetDetails/v1/";

impl SteamWebApiClient {
    pub fn new(webapi_key: impl AsRef<str>) -> SteamWebApiClient {
        SteamWebApiClient {
            client: reqwest::Client::new(),
            webapi_key: webapi_key.as_ref().to_string(),
        }
    }

    pub async fn get_published_file_details(&self, file_ids: impl Iterator<Item = impl AsRef<str>>) -> Result<HashMap<String, GetPublishedFileDetailsResponseItem>> {
        let mut builder = self.client.request(Method::GET, STEAM_WEBAPI_GETDETAILS_URL)
            .query(&[
                ("key", self.webapi_key.as_str()),
                ("includechildren", "true"),
                ("short_description", "true"),
                ("appid", STELLARIS_APPID),
            ]);
        let mut i = 0;
        for file_id in file_ids {
            builder = builder.query(&[(format!("publishedfileids[{}]", i), file_id.as_ref())]);
            i += 1;
        }
        let req = builder.build()?;
        trace!("Request to SteamApi:");
        trace!("{}", req.url());
        let resp = self.client.execute(req).await?.error_for_status()?;
        let text = resp.text().await?;
        trace!("Response from SteamApi:");
        trace!("{}", text);
        Ok(serde_json::from_str::<GetPublishedFileDetailsResponse>(&text)?.response.publishedfiledetails.into_iter()
            .map(|d| {
                match d {
                    GetPublishedFileDetailsResponseItem::FileDetails(ref fd) => (fd.publishedfileid.clone(), d.clone()),
                    GetPublishedFileDetailsResponseItem::MissingItem { result: 9, publishedfileid: ref id } => (id.clone(), d.clone()),
                    _ => panic!("bad deserialisation result"),
                }
            })
            .collect())
    }
}
