use clap::{Parser, Subcommand};
use reqwest::blocking::{Client as HttpClient, ClientBuilder as HttpClientBuilder};
use serde::Deserialize;
use url::Url;
use std::collections::VecDeque;

const BASE_URL: &str = "https://tube.switch.ch";

#[derive(Deserialize)]
struct Video {
    id: String,
    title: Option<String>,
}

impl std::fmt::Display for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.title.as_ref().unwrap_or(&self.id))
    }
}

#[derive(Deserialize)]
struct VideoVariant {
    path: String,
    media_type: String,
}

#[derive(Parser)]
#[clap(author, version)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Download {
        #[clap(value_parser)]
        url: Url,

        #[clap(long, env = "TOKEN")]
        token: String,
    },
}

fn download_video(http_client: &HttpClient, video: &Video) -> Result<(), ()> {
    println!("\n{video}");
    println!("- fetching details");

    let video_id = &video.id;
    let request_url = format!("{BASE_URL}/api/v1/browse/videos/{video_id}/video_variants");

    let video_variant = http_client
        .get(request_url)
        .send()
        .map_err(|_| ())?
        .json::<VecDeque<VideoVariant>>()
        .map_err(|_| ())?
        .pop_front()
        .ok_or(())?;

    println!("- downloading video");

    let video_variant_path = video_variant.path;
    let request_url = format!("{BASE_URL}{video_variant_path}");
    let mut content = http_client.get(request_url).send().map_err(|_| ())?;

    println!("- saving to file");
    
    let extension = video_variant.media_type.split_once('/').ok_or(())?.1;
    let file_name = format!("{video}.{extension}").replace('/', " - ");

    let mut file = std::fs::File::create(file_name).map_err(|_| ())?;
    std::io::copy(&mut content, &mut file).map_err(|_| ())?;

    Ok(())
}

fn download_channel(http_client: &HttpClient, id: &str) {
    println!("Fetching videos in channel");

    let request_url = format!("{BASE_URL}/api/v1/browse/channels/{id}/videos");

    let videos: Vec<Video> = http_client
        .get(request_url)
        .send()
        .unwrap()
        .json()
        .unwrap();

    for video in videos {
        if download_video(http_client, &video).is_err() {
            println!("- download failed");
        }
    }
}

fn download(url: &Url, token: &str) {
    let mut headers = reqwest::header::HeaderMap::new();

    let authorization_header = format!("Token {token}").parse().unwrap();
    headers.insert("Authorization", authorization_header);

    let http_client = HttpClientBuilder::new()
        .default_headers(headers)
        .build()
        .unwrap();

    let mut path_segments = url.path_segments().unwrap();

    match path_segments.next() {
        Some("channels") => match path_segments.next() {
            Some(id) => download_channel(&http_client, id),
            None => println!("Not supported"),
        },
        _ => println!("Not supported"),
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Download { url, token } => download(&url, &token),
    }
}
