use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;

#[derive(Parser, Debug)]
struct Args {
    xspf: std::path::PathBuf,
    name: Option<String>
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename = "playlist")]
struct Playlist {
    title: String,
    tracklist: Tracklist,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename = "tracklist")]
struct Tracklist {
    #[serde(rename = "track")]
    tracks: Vec<Track>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(rename = "track")]
struct Track {
    title: String,
    creator: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct SubsonicTrack {
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

struct SubsonicClient {
    root: String,
    user: String,
    password: String,
    version: String,
    client: String,
}

impl SubsonicClient {
    fn send_request(&self, api: &str, query_params: &str) -> Result<reqwest::blocking::Response> {
        let url = format!("{}{}?u={}&p={}&v={}&c={}&f=json&{}", self.root, api, self.user, self.password, self.version, self.client, query_params);
        Ok(reqwest::blocking::get(url)?)
    }

    fn resolve(&self, track: &Track) -> Option<SubsonicTrack> {
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

    fn create_playlist(&self, name: String, tracks: Vec<SubsonicTrack>) -> Result<()> {
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

fn main() {
    let args = Args::parse();
    env_logger::init();

    let xspf_string = std::fs::read_to_string(args.xspf).unwrap();
    let pl: Playlist = serde_xml_rs::from_str(&xspf_string).unwrap();
    let pl_name = args.name.unwrap_or(pl.title.clone());

    info!("Read total {} tracks in the file", pl.tracklist.tracks.len());

    let ss_client = SubsonicClient {
        root: format!("{}/rest", std::env::var("SS_HOST").expect("SS_HOST not set")),
        user: std::env::var("SS_USER").expect("SS_USER not set"),
        version: "1.12.0".to_string(),
        client: "mbzlists-resolvers".to_string(),
        password: urlencoding::encode(&std::env::var("SS_PASS").expect("SS_PASS not set")).to_string(),
    };

    let mut ss_tracks = vec![];
    for track in &pl.tracklist.tracks {
        match ss_client.resolve(track) {
            Some(ss_track) => ss_tracks.push(ss_track),
            None => info!("Unable to resolve {:?}", track)
        }
    }

    info!("Resolved total {} tracks", ss_tracks.len());
    ss_client.create_playlist(pl_name.clone(), ss_tracks).unwrap();
    info!("Created playlist: {pl_name}");
}
