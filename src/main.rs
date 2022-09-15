use clap::{Parser, Subcommand};
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::{Client as HttpClient, ClientBuilder as HttpClientBuilder};
use serde::Deserialize;
use std::collections::VecDeque;
use std::io::Write;
use std::process::exit;
use url::Url;

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

fn create_progress_bar(title: String, size: u64) -> ProgressBar {
    let style = ProgressStyle::with_template("\n{msg}\n[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .unwrap()
        .progress_chars("#>-");

    ProgressBar::new(size).with_message(title).with_style(style)
}

async fn download_video(
    http_client: &HttpClient,
    video: &Video,
    progress_bar_container: &MultiProgress,
) -> Result<(), ()> {
    let video_variant = http_client
        .get(format!(
            "{BASE_URL}/api/v1/browse/videos/{}/video_variants",
            video.id
        ))
        .send()
        .await
        .map_err(|_| ())?
        .json::<VecDeque<VideoVariant>>()
        .await
        .map_err(|_| ())?
        .pop_front()
        .ok_or(())?;

    let response = http_client
        .get(format!("{BASE_URL}{}", video_variant.path))
        .send()
        .await
        .map_err(|_| ())?;

    let total_size = response.content_length().ok_or(())?;
    let progress_bar =
        progress_bar_container.add(create_progress_bar(video.to_string(), total_size));

    let extension = video_variant.media_type.split_once('/').ok_or(())?.1;
    let file_name = format!("{video}.{extension}").replace('/', " - ");

    let mut file = std::fs::File::create(file_name).map_err(|_| ())?;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|_| ())?;
        file.write_all(&chunk).map_err(|_| ())?;
        progress_bar.inc(chunk.len().try_into().unwrap());
    }

    Ok(())
}

async fn download_channel(http_client: &HttpClient, id: &str) {
    let request_url = format!("{BASE_URL}/api/v1/browse/channels/{id}/videos");

    let videos: Vec<Video> = http_client
        .get(request_url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let progress_bar_container = MultiProgress::new();

    let progress_bar = progress_bar_container.add(
        ProgressBar::new(videos.len().try_into().unwrap()).with_style(
            ProgressStyle::with_template("Downloading channel\n{wide_bar} {pos}/{len}").unwrap(),
        ),
    );

    progress_bar.set_position(0);

    let mut failed_downloads = Vec::new();
    for video in videos {
        if download_video(http_client, &video, &progress_bar_container)
            .await
            .is_err()
        {
            failed_downloads.push(video);
        }

        progress_bar.inc(1);
    }

    progress_bar.finish_with_message("Download complete");

    if !failed_downloads.is_empty() {
        println!("The following videos failed to download:\n");

        for video in failed_downloads {
            println!("{video}");
        }

        exit(1);
    }
}

async fn download(url: &Url, token: &str) {
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
            Some(id) => download_channel(&http_client, id).await,
            None => println!("Not supported"),
        },
        _ => println!("Not supported"),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Download { url, token } => download(&url, &token).await,
    }
}
