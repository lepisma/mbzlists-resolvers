use actix_session::Session;
use actix_web::{get, web, HttpResponse, Responder};
use serde::Deserialize;
use url::Url;
use askama::Template;

use crate::webapp::{PlCreatePageTemplate, PlCreatedPageTemplate};


#[derive(Deserialize)]
struct LoginQuery {
    mbzlists_url: Option<String>,
}

#[get("/youtube/login")]
pub async fn login(query: web::Query<LoginQuery>, session: Session) -> impl Responder {
    let client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap();
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI").unwrap();

    if let Some(mbzlists_url) = &query.mbzlists_url {
        session.insert("mbzlists_url", mbzlists_url).unwrap();
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
    )
    .unwrap();

    HttpResponse::Found()
        .append_header(("Location", auth_url.to_string()))
        .finish()
}

#[derive(Deserialize)]
struct AuthQuery {
    code: String,
}

#[get("/youtube/callback")]
pub async fn callback(query: web::Query<AuthQuery>, session: Session) -> impl Responder {
    let access_token = get_access_token(&query.code).await.unwrap();
    session.insert("access_token", &access_token).unwrap();

    if let Some(mbzlists_url) = session.get::<String>("mbzlists_url").unwrap_or(None) {
        let create_url = format!("/youtube/create?mbzlists_url={}", mbzlists_url);
        return HttpResponse::Found().append_header(("Location", create_url)).finish();
    }

    let body = (PlCreatePageTemplate {
        app_name: "YouTube",
        app_slug: "youtube",
    })
        .render()
        .unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[derive(Deserialize)]
struct CreateQuery {
    mbzlists_url: String,
}

#[get("/youtube/create")]
pub async fn create(query: web::Query<CreateQuery>, session: Session) -> impl Responder {
    let mbzlists_url = &query.mbzlists_url;
    let access_token: Option<String> = session.get("access_token").unwrap_or(None);

    if access_token.is_none() {
        return HttpResponse::Found()
            .append_header(("Location", format!("/youtube/login?mbzlists_url={}", mbzlists_url)))
            .finish();
    }

    let access_token = access_token.unwrap();

    let playlist = crate::mbzlists::Playlist::from_url(mbzlists_url.clone()).await.unwrap();

    let yt_playlist_id = create_yt_playlist(&playlist.title, &access_token).await.unwrap();

    for track in playlist.tracklist.tracks {
        if let Some(video_id) = search_youtube(&track.title, &track.creator, &access_token).await {
            add_video_to_playlist(&yt_playlist_id, &video_id, &access_token).await.unwrap();
        }
    }

    let playlist_url = format!("https://www.youtube.com/playlist?list={}", yt_playlist_id);
    let body = (PlCreatedPageTemplate {
        app_name: "YouTube",
        playlist_url: &playlist_url,
    })
        .render()
        .unwrap();

    HttpResponse::Ok().content_type("text/html").body(body)
}

async fn get_access_token(code: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client_id = std::env::var("GOOGLE_CLIENT_ID")?;
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")?;
    let redirect_uri = std::env::var("GOOGLE_REDIRECT_URI")?;

    let params = [
        ("code", code),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
        ("redirect_uri", &redirect_uri),
        ("grant_type", "authorization_code"),
    ];

    let resp = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(resp["access_token"].as_str().unwrap().to_string())
}

async fn create_yt_playlist(title: &str, access_token: &str) -> Result<String, Box<dyn std::error::Error>> {
    let body = serde_json::json!({
        "snippet": {
            "title": title,
            "description": "Imported from mbzlists"
        },
        "status": {
            "privacyStatus": "private"
        }
    });

    let resp = reqwest::Client::new()
        .post("https://www.googleapis.com/youtube/v3/playlists?part=snippet,status")
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(resp["id"].as_str().unwrap().to_string())
}

async fn search_youtube(title: &str, artist: &str, access_token: &str) -> Option<String> {
    let query = format!("{} {}", title, artist);
    let url = format!(
        "https://www.googleapis.com/youtube/v3/search?part=snippet&type=video&q={}",
        urlencoding::encode(&query)
    );

    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?
        .json::<serde_json::Value>()
        .await
        .ok()?;

    resp["items"]
        .get(0)?
        .get("id")?
        .get("videoId")?
        .as_str()
        .map(|s| s.to_string())
}

async fn add_video_to_playlist(playlist_id: &str, video_id: &str, access_token: &str) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::json!({
        "snippet": {
            "playlistId": playlist_id,
            "resourceId": {
                "kind": "youtube#video",
                "videoId": video_id
            }
        }
    });

    reqwest::Client::new()
        .post("https://www.googleapis.com/youtube/v3/playlistItems?part=snippet")
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await?;

    Ok(())
}
