use actix_session::Session;
use actix_web::{get, http::StatusCode, web, HttpResponse, Responder};
use log::debug;
use url::Url;
use anyhow::Result;
use askama::Template;

use crate::mbzlists::{self, Track};


#[derive(Template)]
#[template(path = "sp_upload.html")]
struct SpotifyUploadPageTemplate {}

#[derive(Template)]
#[template(path = "sp_created.html")]
struct SpotifyCreatedPageTemplate<'a> {
    playlist_url: &'a str,
}

const API_ROOT: &str = "https://api.spotify.com/v1";

#[derive(serde::Deserialize)]
struct LoginQuery {
    mbzlists_url: Option<String>,
}

#[get("/spotify/login")]
pub async fn login(query: web::Query<LoginQuery>, session: Session) -> impl Responder {
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").unwrap();
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI").unwrap();

    if let Some(mbzlists_url) = &query.mbzlists_url {
        // If the user is coming here with a url already, save that in the
        // session so that we can bypass the input form
        session.insert("mbzlists_url", mbzlists_url).unwrap();
    }

    let auth_url = Url::parse_with_params(
        "https://accounts.spotify.com/authorize",
        &[
            ("client_id", &client_id),
            ("response_type", &"code".to_string()),
            ("redirect_uri", &redirect_uri),
            ("scope", &"playlist-modify-private playlist-modify-public".to_string()),
        ],
    ).unwrap();

    HttpResponse::Found().append_header(("Location", auth_url.to_string())).finish()
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct SpotifyAlbum {
    id: String,
    name: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct SpotifyArtist {
    id: String,
    name: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct SpotifyTrack {
    id: String,
    name: String,
    artists: Vec<SpotifyArtist>,
    album: SpotifyAlbum,
}

#[derive(serde::Deserialize, Debug)]
struct SpotifyAPIError {
    status: usize,
    message: String,
}


#[derive(serde::Deserialize, Debug)]
struct TracksSearchResult {
    total: usize,
    items: Vec<SpotifyTrack>,
}

#[derive(serde::Deserialize, Debug)]
#[serde(untagged)]
enum SpotifyResponse {
    Success { tracks: TracksSearchResult },
    Error { error: SpotifyAPIError },
}

pub struct SpotifyPlaylist {
    id: String,
    url: String,
}

async fn resolve(track: &Track, access_token: &str) -> Option<SpotifyTrack> {
    let client = reqwest::Client::new();
    let query = urlencoding::encode(&format!("{} artist:{}", track.title, track.creator)).to_string();

    let search_resp = client
        .get(format!("{API_ROOT}/search?q={query}&type=track"))
        .bearer_auth(&access_token)
        .send()
        .await
        .unwrap()
        .json::<SpotifyResponse>()
        .await
        .unwrap();

    match search_resp {
        SpotifyResponse::Success { tracks } => {
            let found_track = tracks.items[0].clone();

            // We are letting go of case sensitive matching here. This might be
            // a problem later though.
            if found_track.name.to_lowercase() == track.title.to_lowercase() && found_track.artists[0].name.to_lowercase() == track.creator.to_lowercase() {
                Some(found_track)
            } else {
                debug!("Error in matching: {:?}", found_track);
                None
            }
        },
        SpotifyResponse::Error { error } => {
            debug!("{:?}", error);
            None
        },
    }
}

async fn create_playlist(name: &str, tracks: Vec<SpotifyTrack>, user_id: &str, access_token: &str) -> Result<SpotifyPlaylist> {
    let client = reqwest::Client::new();

    let playlist_resp = client
        .post(format!("{API_ROOT}/users/{user_id}/playlists"))
        .bearer_auth(&access_token)
        .json(&serde_json::json!({
            "name": name,
            "public": false,
            "description": "Imported from mbzlists"
        }))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let playlist_id = playlist_resp["id"].as_str().unwrap();
    let playlist_url = playlist_resp["external_urls"]["spotify"].as_str().unwrap();

    client.post(format!("{API_ROOT}/playlists/{playlist_id}/tracks"))
        .bearer_auth(&access_token)
        .json(&serde_json::json!({
            "uris": tracks.iter().map(|t| format!("spotify:track:{}", t.id)).collect::<Vec<String>>()
        }))
        .send()
        .await?;

    Ok(SpotifyPlaylist { id: playlist_id.to_string(), url: playlist_url.to_string() })
}

async fn get_access_token(auth_code: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").unwrap();
    let client_secret = std::env::var("SPOTIFY_CLIENT_SECRET").unwrap();
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI").unwrap();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", auth_code),
        ("redirect_uri", &redirect_uri),
    ];

    let auth_header = base64::encode(format!("{}:{}", client_id, client_secret));
    let token_resp = client
        .post("https://accounts.spotify.com/api/token")
        .header("Authorization", format!("Basic {}", auth_header))
        .form(&params)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    Ok(token_resp["access_token"].as_str().unwrap().to_string())
}

// Return Spotify ID for the current logged in user
async fn get_current_user_id(access_token: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let user_resp = client
        .get(format!("{API_ROOT}/me"))
        .bearer_auth(access_token)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    Ok(user_resp["id"].as_str().unwrap().to_string())
}

#[derive(serde::Deserialize)]
struct AuthQuery {
    code: String,
}

#[get("/spotify/callback")]
pub async fn callback(query: web::Query<AuthQuery>, session: Session) -> impl Responder {
    let access_token = get_access_token(&query.code).await.unwrap();
    let user_id = get_current_user_id(&access_token).await.unwrap();

    session.insert("access_token", &access_token).unwrap();
    session.insert("user_id", &user_id).unwrap();

    // If an mbzlists url is saved in session, we will directly use that, else
    // will ask user to input the url via a form.
    if let Some(mbzlists_url) = session.get::<String>("mbzlists_url").unwrap_or(None) {
        let create_url = format!("/spotify/create?mbzlists_url={}", mbzlists_url);
        return HttpResponse::Found().append_header(("Location", create_url)).finish();
    }

    let body = (SpotifyUploadPageTemplate {}).render().unwrap();
    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body)
}

#[derive(serde::Deserialize)]
struct CreateQuery {
    mbzlists_url: String,
}

#[get("/spotify/create")]
pub async fn create(query: web::Query<CreateQuery>, session: Session) -> impl Responder {
    let mbzlists_url = query.mbzlists_url.clone();
    let access_token: Option<String> = session.get("access_token").unwrap_or(None);
    let user_id: Option<String> = session.get("user_id").unwrap_or(None);

    if access_token.is_none() || user_id.is_none() {
        return HttpResponse::Found()
            .append_header(("Location", format!("/spotify/login?mbzlists_url={mbzlists_url}")))
            .finish();
    }

    let access_token = access_token.unwrap();
    let user_id = user_id.unwrap();

    let playlist = mbzlists::Playlist::from_url(mbzlists_url).await.unwrap();

    let mut sp_tracks = Vec::new();

    for track in playlist.tracklist.tracks {
        if let Some(ss_track) = resolve(&track, &access_token).await {
            sp_tracks.push(ss_track);
        }
    }

    let spotify_playlist = create_playlist(&playlist.title, sp_tracks, &user_id, &access_token).await.unwrap();

    let body = (SpotifyCreatedPageTemplate { playlist_url: &spotify_playlist.url }).render().unwrap();

    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body)
}
