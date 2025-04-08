use actix_web::{web, App, HttpServer, HttpResponse, Responder, get};
use url::Url;


#[get("/spotify")]
async fn home() -> impl Responder {
    HttpResponse::Found().append_header(("Location", "https://mbzlists.com".to_string())).finish()
}

#[get("/spotify/login")]
async fn login() -> impl Responder {
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").unwrap();
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI").unwrap();

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

#[derive(serde::Deserialize)]
struct AuthQuery {
    code: String,
}

#[get("/spotify/callback")]
async fn callback(query: web::Query<AuthQuery>) -> impl Responder {
    let client = reqwest::Client::new();
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").unwrap();
    let client_secret = std::env::var("SPOTIFY_CLIENT_SECRET").unwrap();
    let redirect_uri = std::env::var("SPOTIFY_REDIRECT_URI").unwrap();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", &query.code),
        ("redirect_uri", &redirect_uri),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
    ];

    let token_resp = client
        .post("https://accounts.spotify.com/api/token")
        .form(&params)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let access_token = token_resp["access_token"].as_str().unwrap();

    let user_resp = client
        .get("https://api.spotify.com/v1/me")
        .bearer_auth(access_token)
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let user_id = user_resp["id"].as_str().unwrap();

    let playlist_resp = client
        .post(format!("https://api.spotify.com/v1/users/{}/playlists", user_id))
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "name": "Rusty Playlist",
            "public": false,
            "description": "Created from Rust"
        }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let url = playlist_resp["external_urls"]["spotify"].as_str().unwrap();
    HttpResponse::Ok().body(format!("✅ Playlist created: <a href='{}'>{}</a>", url, url))
}

pub async fn serve() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(home)
            .service(login)
            .service(callback)
    })
    .bind(("127.0.0.1", 8888))?
    .run()
    .await
}
