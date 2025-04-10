use anyhow::{anyhow, Result};
use url::Url;

#[derive(serde::Deserialize, Debug)]
#[serde(rename = "playlist")]
pub struct Playlist {
    pub title: String,
    pub tracklist: Tracklist,
}

impl Playlist {
    pub fn from_xspf(file: std::path::PathBuf) -> Result<Playlist> {
        let xspf_string = std::fs::read_to_string(file)?;
        Ok(serde_xml_rs::from_str(&xspf_string)?)
    }

    pub async fn from_url(url: &str) -> Result<Playlist> {
        let parsed = Url::parse(url)?;
        let host = parsed.host_str();

        match parsed.path_segments().unwrap().last() {
            Some(view_id) => Playlist::from_view_id(view_id, host).await,
            None => Err(anyhow!("Malformed url: {url}"))
        }
    }

    pub async fn from_view_id(view_id: &str, host: Option<&str>) -> Result<Playlist> {
        let host = host.unwrap_or("mbzlists.com");
        let response = reqwest::get(format!("https://{host}/api/list/{view_id}?type=xspf")).await?;
        let bytes = response.bytes().await?;
        let body = String::from_utf8(bytes.to_vec()).expect("body is not valid UTF8!");

        Ok(serde_xml_rs::from_str(&body)?)
    }
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename = "tracklist")]
pub struct Tracklist {
    #[serde(rename = "track")]
    pub tracks: Vec<Track>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename = "track")]
pub struct Track {
    pub title: String,
    pub creator: String,
}
