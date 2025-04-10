use actix_session::Session;
use actix_web::{get, web, error, HttpResponse, Responder};
use serde::Deserialize;
use url::Url;
use askama::Template;
use anyhow::{Context, Result, anyhow};

use crate::webapp::{PlCreatePageTemplate, PlCreatedPageTemplate};


#[derive(Deserialize)]
struct LoginQuery {
    mbzlists_url: Option<String>,
}

#[get("/youtube/login")]
pub async fn login(query: web::Query<LoginQuery>, session: Session) -> Result<impl Responder, error::Error> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Missing GOOGLE_CLIENT_ID env variable"))
    })?;

    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI").map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Missing GOOGLE_REDIRECT_URL env variable"))
    })?;

    if let Some(mbzlists_url) = &query.mbzlists_url {
        session.insert("mbzlists_url", mbzlists_url).map_err(|_| {
            error::ErrorInternalServerError(anyhow!("Unable to set session variable `mbzlists_url`"))
        })?;
    }

    let auth_url = Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/auth",
        &[
            ("client_id", &client_id),
            ("response_type", &"code".to_string()),
            ("redirect_uri", &redirect_uri),
            ("scope", &"https://www.googleapis.com/auth/youtube".to_string()),
            ("access_type", &"offline".to_string()),
            ("prompt", &"consent".to_string()),
        ],
    ).map_err(|_| {
        error::ErrorInternalServerError(anyhow!("Unable to create auth_url"))
    })?;

    Ok(HttpResponse::Found()
        .append_header(("Location", auth_url.to_string()))
        .finish())
}

#[derive(Deserialize)]
struct AuthQuery {
    code: String,
}

#[get("/youtube/callback")]
pub async fn callback(query: web::Query<AuthQuery>, session: Session) -> Result<impl Responder, error::Error> {
    let access_token = get_access_token(&query.code).await.unwrap();
    session.insert("access_token", &access_token).unwrap();

    if let Some(mbzlists_url) = session.get::<String>("mbzlists_url").unwrap_or(None) {
        let create_url = format!("/youtube/create?mbzlists_url={}", mbzlists_url);
        return Ok(HttpResponse::Found().append_header(("Location", create_url)).finish());
    }

    let body = (PlCreatePageTemplate {
        app_name: "YouTube",
        app_slug: "youtube",
    })
        .render()
        .unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

#[derive(Deserialize)]
struct CreateQuery {
    mbzlists_url: String,
}

#[get("/youtube/create")]
pub async fn create(query: web::Query<CreateQuery>, session: Session) -> Result<impl Responder, error::Error> {
    let mbzlists_url = &query.mbzlists_url;
    let access_token: Option<String> = session.get("access_token").unwrap_or(None);

    if access_token.is_none() {
        return Ok(HttpResponse::Found()
            .append_header(("Location", format!("/youtube/login?mbzlists_url={}", mbzlists_url)))
            .finish());
    }

    let access_token = access_token.unwrap();

    let playlist = crate::mbzlists::Playlist::from_url(mbzlists_url.clone()).await.map_err(error::ErrorInternalServerError)?;
    let yt_playlist_id = create_yt_playlist(&playlist.title, &access_token).await.map_err(error::ErrorInternalServerError)?;

    for track in playlist.tracklist.tracks {
        match search_youtube(&track.title, &track.creator, &access_token).await {
            Ok(video_id) => add_video_to_playlist(&yt_playlist_id, &video_id, &access_token).await.map_err(error::ErrorInternalServerError)?,
            // This is a little aggressive, but there are very less chance of a
            // youtube search not returning anything in normal cases
            Err(err) => return Err(error::ErrorInternalServerError(err))
        }
    }

    let playlist_url = format!("https://www.youtube.com/playlist?list={}", yt_playlist_id);
    let body = (PlCreatedPageTemplate {
        app_name: "YouTube",
        playlist_url: &playlist_url,
    })
        .render()
        .unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(body))
}

async fn get_access_token(code: &str) -> Result<String> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").context("Missing GOOGLE_CLIENT_ID env variable")?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").context("Missing GOOGLE_CLIENT_SECRET env variable")?;
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI").context("Missing GOOGLE_REDIRECT_URI env variable")?;

    let params = [
        ("code", code),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
        ("redirect_uri", &redirect_uri),
        ("grant_type", "authorization_code"),
    ];

    let client = reqwest::Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
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
        .ok_or_else(|| anyhow!("Missing access_token in response"))?;

    Ok(token.to_string())
}

async fn create_yt_playlist(title: &str, access_token: &str) -> Result<String> {
    let body = serde_json::json!({
        "snippet": {
            "title": title,
            "description": "Imported from mbzlists"
        },
        "status": {
            "privacyStatus": "private"
        }
    });

    let client = reqwest::Client::new();
    let res = client
        .post("https://www.googleapis.com/youtube/v3/playlists?part=snippet,status")
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .context("Failed to send create playlist request")?;

    let status = res.status();
    let body_text = res.text().await.context("Failed to read playlist response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("YouTube playlist creation failed: {} - {}", status, body_text));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body_text).context("Failed to parse playlist JSON response")?;

    let playlist_id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing playlist ID in response"))?;

    Ok(playlist_id.to_string())
}

async fn search_youtube(title: &str, artist: &str, access_token: &str) -> Result<String> {
    let query = format!("{} {}", title, artist);
    let url = format!(
        "https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&q={}",
        urlencoding::encode(&query)
    );

    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("Failed to send search request")?;

    let status = res.status();
    let body_text = res.text().await.context("Failed to read search response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("YouTube search failed: {} - {}", status, body_text));
    }

    let json: serde_json::Value =
        serde_json::from_str(&body_text).context("Failed to parse search JSON response")?;

    let video_id = json["items"]
        .get(0)
        .and_then(|item| item.get("id"))
        .and_then(|id| id.get("videoId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No videoId found in search results"))?;

    Ok(video_id.to_string())
}

async fn add_video_to_playlist(playlist_id: &str, video_id: &str, access_token: &str) -> Result<()> {
    let body = serde_json::json!({
        "snippet": {
            "playlistId": playlist_id,
            "resourceId": {
                "kind": "youtube#video",
                "videoId": video_id
            }
        }
    });

    let client = reqwest::Client::new();
    let res = client
        .post("https://www.googleapis.com/youtube/v3/playlistItems?part=snippet")
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await
        .context("Failed to send add-to-playlist request")?;

    let status = res.status();
    let body_text = res.text().await.context("Failed to read add-to-playlist response body")?;

    if status != reqwest::StatusCode::OK {
        return Err(anyhow!("Failed to add video to playlist: {} - {}", status, body_text));
    }

    Ok(())
}
