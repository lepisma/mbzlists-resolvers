use actix_session::Session;
use actix_web::{get, web, error, HttpResponse, Responder};
use log::debug;
use url::Url;
use anyhow::{Result, Context, anyhow};
use askama::Template;

use crate::{mbzlists::{self, Track}, webapp::{PlCreatePageTemplate, PlCreatedPageTemplate}};


const API_ROOT: &str = "https://api.spotify.com/v1";

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

impl std::fmt::Display for SpotifyAPIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Spotify API error ({}): {}", self.status, self.message)
    }
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

async fn resolve(track: &Track, access_token: &str) -> Result<SpotifyTrack> {
    let query = urlencoding::encode(&format!("{} artist:{}", track.title, track.creator)).to_string();

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{API_ROOT}/search?q={query}&type=track"))
        .bearer_auth(&access_token)
        .send()
        .await
        .context("Failed to send search request")?;

    let status = res.status();
    let body = res.text().await.context("Failed to read search response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("Spotify search failed: {} - {}", status, body));
    }

    let json: SpotifyResponse = serde_json::from_str(&body).context("Failed to parse search JSON response")?;

    match json {
        SpotifyResponse::Success { tracks } => {
            let found_track = tracks.items[0].clone();

            // We are letting go of case sensitive matching here. This might be
            // a problem later though.
            if found_track.name.to_lowercase() == track.title.to_lowercase() && found_track.artists[0].name.to_lowercase() == track.creator.to_lowercase() {
                Ok(found_track)
            } else {
                debug!("Error in matching: {:?}", found_track);
                Err(anyhow!("Error in matching: {:?}", found_track))
            }
        },
        SpotifyResponse::Error { error } => {
            debug!("{:?}", error);
            anyhow::bail!(error);
        },
    }
}

async fn create_playlist(name: &str, tracks: Vec<SpotifyTrack>, user_id: &str, access_token: &str) -> Result<SpotifyPlaylist> {
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{API_ROOT}/users/{user_id}/playlists"))
        .bearer_auth(&access_token)
        .json(&serde_json::json!({
            "name": name,
            "public": false,
            "description": "Imported from mbzlists"
        }))
        .send()
        .await
        .context("Failed to send create playlist request")?;

    let status = res.status();
    let body = res.text().await.context("Failed to read playlist response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("Spotify playlist creation failed: {} - {}", status, body));
    }

    let json: serde_json::Value =serde_json::from_str(&body).context("Failed to parse playlist JSON response")?;

    let playlist_id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing playlist ID in response: {}", json))?;

    let playlist_url = json
        .get("external_urls")
        .and_then(|v| v.get("spotify"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing playlist url in response: {}", json))?;

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
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").context("Missing SPOTIFY_CLIENT_ID env variable")?;
    let client_secret = std::env::var("SPOTIFY_CLIENT_SECRET").context("Missing SPOTIFY_CLIENT_SECRET env variable")?;
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI").context("Missing SPOTIFY_REDIRECT_URI env variable")?;

    let params = [
        ("grant_type", "authorization_code"),
        ("code", auth_code),
        ("redirect_uri", &redirect_uri),
    ];

    let client = reqwest::Client::new();
    let auth_header = base64::encode(format!("{}:{}", client_id, client_secret));

    let res = client
        .post("https://accounts.spotify.com/api/token")
        .header("Authorization", format!("Basic {}", auth_header))
        .form(&params)
        .send()
        .await
        .context("Failed to send token request")?;

    let status = res.status();
    let body = res.text().await.context("Failed to read response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("Token exchange failed: {} - {}", status, body));
    }

    let json: serde_json::Value = serde_json::from_str(&body).context("Failed to parse JSON response")?;

    let token = json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing access_token in response: {}", json))?;

    Ok(token.to_string())
}

// Return Spotify ID for the current logged in user
async fn get_current_user_id(access_token: &str) -> Result<String> {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{API_ROOT}/me"))
        .bearer_auth(access_token)
        .send()
        .await
        .context("Failed to send user id request")?;

    let status = res.status();
    let body = res.text().await.context("Failed to read response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("User id request failed: {} - {}", status, body));
    }

    let json: serde_json::Value = serde_json::from_str(&body).context("Failed to parse JSON response")?;

    let user_id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing id in response: {}", json))?;

    Ok(user_id.to_string())
}

#[derive(serde::Deserialize)]
struct LoginQuery {
    mbzlists_url: Option<String>,
}

#[get("/spotify/login")]
pub async fn login(query: web::Query<LoginQuery>, session: Session) -> Result<impl Responder, error::Error> {
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Missing SPOTIFY_CLIENT_ID env variable"))
    })?;
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI").map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Missing SPOTIFY_REDIRECT_URI env variable"))
    })?;

    if let Some(mbzlists_url) = &query.mbzlists_url {
        // If the user is coming here with a url already, save that in the
        // session so that we can bypass the input form
        session.insert("mbzlists_url", mbzlists_url).map_err(|_| {
            error::ErrorInternalServerError(anyhow!("Unable to set session variable `mbzlists_url`"))
        })?;
    }

    let auth_url = Url::parse_with_params(
        "https://accounts.spotify.com/authorize",
        &[
            ("client_id", &client_id),
            ("response_type", &"code".to_string()),
            ("redirect_uri", &redirect_uri),
            ("scope", &"playlist-modify-private playlist-modify-public".to_string()),
        ],
    ).map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Unable to create auth_url"))
    })?;

    Ok(HttpResponse::Found()
        .append_header(("Location", auth_url.to_string()))
        .finish())
}

#[derive(serde::Deserialize)]
struct AuthQuery {
    code: String,
}

#[get("/spotify/callback")]
pub async fn callback(query: web::Query<AuthQuery>, session: Session) -> Result<impl Responder, error::Error> {
    let access_token = get_access_token(&query.code).await.map_err(error::ErrorInternalServerError)?;
    let user_id = get_current_user_id(&access_token).await.map_err(error::ErrorInternalServerError)?;

    session.insert("access_token", &access_token).map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Unable to set session variable `access_token`"))
    })?;
    session.insert("user_id", &user_id).map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Unable to set session variable `user_id`"))
    })?;

    // If an mbzlists url is saved in session, we will directly use that, else
    // will ask user to input the url via a form.
    if let Some(mbzlists_url) = session.get::<String>("mbzlists_url").unwrap_or(None) {
        let create_url = format!("/spotify/create?mbzlists_url={}", mbzlists_url);
        return Ok(HttpResponse::Found().append_header(("Location", create_url)).finish());
    }

    let body = (PlCreatePageTemplate {
        app_name: "Spotify",
        app_slug: "spotify",
    })
        .render()
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[derive(serde::Deserialize)]
struct CreateQuery {
    mbzlists_url: String,
}

#[get("/spotify/create")]
pub async fn create(query: web::Query<CreateQuery>, session: Session) -> Result<impl Responder, error::Error> {
    let mbzlists_url = query.mbzlists_url.clone();
    let access_token: Option<String> = session.get("access_token").unwrap_or(None);
    let user_id: Option<String> = session.get("user_id").unwrap_or(None);

    if access_token.is_none() || user_id.is_none() {
        return Ok(HttpResponse::Found()
            .append_header(("Location", format!("/spotify/login?mbzlists_url={mbzlists_url}")))
            .finish());
    }

    let access_token = access_token.unwrap();
    let user_id = user_id.unwrap();

    let playlist = mbzlists::Playlist::from_url(&mbzlists_url).await.map_err(error::ErrorInternalServerError)?;

    let mut sp_tracks = Vec::new();

    for track in playlist.tracklist.tracks {
        match resolve(&track, &access_token).await {
            Ok(ss_track) => sp_tracks.push(ss_track),
            Err(_) => {}
        }
    }

    let spotify_playlist = create_playlist(&playlist.title, sp_tracks, &user_id, &access_token).await.map_err(error::ErrorInternalServerError)?;

    let body = (PlCreatedPageTemplate {
        app_name: "Spotify",
        playlist_url: &spotify_playlist.url
    })
        .render()
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}
