use anyhow::{anyhow, Result};
use crate::Track;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct SubsonicTrack {
    id: String,
    title: String,
    artist: String,
}

#[derive(serde::Deserialize, Debug)]
struct SubsonicResponseWrapper {
    #[serde(rename = "subsonic-response")]
    subsonic_response: SubsonicResponse,
}

#[derive(serde::Deserialize, Debug)]
struct SubsonicResponse {
    status: String,
    #[serde(rename = "searchResult2")]
    search_results2: Option<SubsonicSearchResult2>,
}

#[derive(serde::Deserialize, Debug)]
struct SubsonicSearchResult2 {
    song: Option<Vec<SubsonicTrack>>,
}

pub struct SubsonicClient {
    root: String,
    user: String,
    password: String,
    version: String,
    client: String,
}

impl SubsonicClient {
    pub fn new(root: String, user: String, password: String) -> SubsonicClient {
        SubsonicClient {
            root, user, password,
            version: "1.12.0".to_string(),
            client: "mbzlists-resolvers".to_string(),
        }
    }

    fn send_request(&self, api: &str, query_params: &str) -> Result<reqwest::blocking::Response> {
        let url = format!("{}{}?u={}&p={}&v={}&c={}&f=json&{}", self.root, api, self.user, self.password, self.version, self.client, query_params);
        Ok(reqwest::blocking::get(url)?)
    }

    pub fn resolve(&self, track: &Track) -> Option<SubsonicTrack> {
        let query = format!("{} {}", track.title, track.creator);
        let response = self.send_request("/search2", &format!("query={}", urlencoding::encode(&query))).unwrap();
        let output = response.json::<SubsonicResponseWrapper>().unwrap();
        if let Some(SubsonicSearchResult2 { song: Some(ss_tracks) }) = output.subsonic_response.search_results2 {
            if ss_tracks.is_empty() {
                None
            } else {
                let ss_track = ss_tracks[0].clone();
                // Final check to ensure search quality
                if ss_track.title == track.title && ss_track.artist == track.creator {
                    Some(ss_tracks[0].clone())
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    pub fn create_playlist(&self, name: String, tracks: Vec<SubsonicTrack>) -> Result<()> {
        let ids = tracks.iter().map(|t| format!("songId={}", t.id)).collect::<Vec<String>>().join("&");
        let response = self.send_request("/createPlaylist", &format!("name={}&{ids}", urlencoding::encode(&name)))?;
        let output = response.json::<SubsonicResponseWrapper>()?;
        if output.subsonic_response.status == "ok" {
            Ok(())
        } else {
            Err(anyhow!("Got response: {:?}", output))
        }
    }
}
