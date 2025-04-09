use crate::platform::spotify;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, get, http::StatusCode, App, HttpResponse, HttpServer, Responder};

fn generate_home_page() -> String {
    crate::view::generate_page("
  <h1>mbzlists-resolvers</h1>
  <p>Resolvers are tools that map and convert mbzlists entities to equivalent entities on other platforms.</p>

  <div class=\"card\">
    <h2><i>mbzlists → Spotify</i></h2>
    <p>While you can search and play individual songs on Spotify via the mbzlists web app itself, this tool allows you to export an mbzlists playlist to Spotify.</p>
    <a class=\"btn\" href=\"/spotify/login\">Proceed to Login</a>
  </div>

  <div class=\"card\">
    <h2><i>mbzlists → YouTube</i></h2>
    <p>As of now, you can create a temporary playlist from the mbzlists webapp. This is not importable to your account though. That's a work in progress.</p>
  </div>

  <div class=\"card\">
    <h2><i>mbzlists → Subsonic Compatible Server</i></h2>
    <p>You can import xspf files from mbzlists to any subsonic compatible media server using the mbzlists-resolvers command line tool.</p>
    <a class=\"btn\" href=\"https://github.com/lepisma/mbzlists-resolvers\">Open Documentation</a>
  </div>")
}

#[get("/")]
async fn home() -> impl Responder {
    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(generate_home_page())
}

pub async fn serve() -> std::io::Result<()> {
    let secret_key = Key::generate();

    HttpServer::new(move || {
        App::new()
            .wrap(SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone()).build())
            .service(home)
            .service(spotify::login)
            .service(spotify::callback)
            .service(spotify::create)
    })
    .bind(("127.0.0.1", 8888))?
    .run()
    .await
}
