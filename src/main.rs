use clap::{Parser, Subcommand};
use log::info;
use platform::subsonic::SubsonicClient;
use anyhow::Result;

mod platform;
mod webapp;
mod mbzlists;

#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    platform: Platforms,
}

#[derive(Subcommand, Debug)]
enum Platforms {
    Subsonic {
        xspf: std::path::PathBuf,
        name: Option<String>,

        #[arg(long)]
        no_create: bool,
    },
    Webapp,
}

#[actix_web::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    env_logger::init();

    match args.platform {
        Platforms::Subsonic { xspf, name, no_create } => {
            let pl = mbzlists::Playlist::from_xspf(xspf)?;
            let pl_name = name.unwrap_or(pl.title.clone());

            info!("Read total {} tracks in the file", pl.tracklist.tracks.len());

            let ss_client = SubsonicClient::new(
                format!("{}/rest", std::env::var("SS_HOST").expect("SS_HOST not set")),
                std::env::var("SS_USER").expect("SS_USER not set"),
                urlencoding::encode(&std::env::var("SS_PASS").expect("SS_PASS not set")).to_string(),
            );

            let mut ss_tracks = vec![];
            for track in &pl.tracklist.tracks {
                match ss_client.resolve(track) {
                    Some(ss_track) => ss_tracks.push(ss_track),
                    None => info!("Unable to resolve {:?}", track)
                }
            }

            info!("Resolved total {} tracks", ss_tracks.len());

            if !ss_tracks.is_empty() && !no_create {
                ss_client.create_playlist(pl_name.clone(), ss_tracks).unwrap();
                info!("Created playlist: {pl_name}");
            }
            Ok(())
        },
        Platforms::Webapp => {
            Ok(webapp::serve().await?)
        }
    }
}
