use std::collections::HashMap;

use reqwest::Method;

use crate::{error::Result, schemas::{GetPublishedFileDetailsResponse, PublishedFileDetails}};

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

    pub async fn get_published_file_details(&self, file_ids: impl Iterator<Item = impl AsRef<str>>) -> Result<HashMap<String, PublishedFileDetails>> {
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
        let resp = builder.send().await?.error_for_status()?.json::<GetPublishedFileDetailsResponse>().await?;
        Ok(resp.response.publishedfiledetails.into_iter().map(|d| (d.publishedfileid.clone(), d)).collect())
    }
}
