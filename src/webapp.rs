use crate::platform::spotify;
use crate::platform::youtube;
use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, get, http::StatusCode, App, HttpResponse, HttpServer, Responder};
use askama::Template;


#[derive(Template)]
#[template(path = "home.html")]
struct HomePageTemplate {}

#[derive(Template)]
#[template(path = "playlist_create.html")]
pub struct PlCreatePageTemplate<'a> {
    pub app_name: &'a str,
    pub app_slug: &'a str,
}

#[derive(Template)]
#[template(path = "playlist_created.html")]
pub struct PlCreatedPageTemplate<'a> {
    pub app_name: &'a str,
    pub playlist_url: &'a str,
}


#[get("/")]
async fn home() -> impl Responder {
    let body = (HomePageTemplate {}).render().unwrap();

    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body)
}

pub async fn serve() -> std::io::Result<()> {
    let secret_key = Key::generate();
    let host = std::env::var("MBZR_HOST").unwrap_or("127.0.0.1".to_string());
    let port = std::env::var("MBZR_PORT").unwrap_or("8888".to_string()).parse::<u16>().unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone()).build())
            .service(home)
            .service(spotify::login)
            .service(spotify::callback)
            .service(spotify::create)
            .service(youtube::login)
            .service(youtube::callback)
            .service(youtube::create)
    })
    .bind((host, port))?
    .run()
    .await
}
