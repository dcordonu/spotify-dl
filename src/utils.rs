use crate::media::media_item::MediaItem;
use crate::media::media_queue::MediaQueue;
use anyhow::Result;
use lazy_static::lazy_static;
use librespot::core::{Session, SpotifyUri};
use regex::Regex;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

lazy_static! {
    static ref SPOTIFY_URL_REGEX: Regex =
        Regex::new(r"https://open\.spotify\.com(?:/intl-[a-z]{2})?/(\w+)/([a-zA-Z0-9]+)").unwrap();
}

pub type AsyncFn<T> =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<T>> + Send>> + Send + Sync>;

#[tracing::instrument(name = "get_tracks", skip(session), level = "debug")]
pub async fn get_tracks(spotify_ids: Vec<String>, session: &Session) -> Result<Vec<MediaItem>> {
    let mut tracks: Vec<MediaItem> = Vec::new();
    for id in spotify_ids {
        tracing::debug!("Getting tracks for: {}", id);
        let uri: SpotifyUri = parse_uri_or_url(&id).ok_or(anyhow::anyhow!("Invalid track"))?;
        let new_tracks = match uri {
            SpotifyUri::Track { .. } | SpotifyUri::Episode { .. } => vec![MediaItem::from(&uri)?],
            SpotifyUri::Album { .. } | SpotifyUri::Playlist { .. } | SpotifyUri::Show { .. } => {
                MediaQueue::from_uri(&uri)?.get_tracks(session).await?
            }
            _ => {
                tracing::warn!("Unsupported item type: {:?}", id);
                vec![]
            }
        };
        tracks.extend(new_tracks);
    }
    tracing::debug!("Got tracks: {:?}", tracks);
    Ok(tracks)
}

pub fn parse_uri_or_url(track: &str) -> Option<SpotifyUri> {
    parse_uri(track).or_else(|| parse_url(track))
}

fn parse_uri(track_uri: &str) -> Option<SpotifyUri> {
    let res = SpotifyUri::from_uri(track_uri);
    tracing::info!("Parsed URI: {:?}", res);
    res.ok()
}

fn parse_url(track_url: &str) -> Option<SpotifyUri> {
    let results = SPOTIFY_URL_REGEX.captures(track_url)?;
    let uri = format!(
        "spotify:{}:{}",
        results.get(1)?.as_str(),
        results.get(2)?.as_str()
    );
    SpotifyUri::from_uri(&uri).ok()
}

pub(crate) fn clean_invalid_characters<S>(input: S) -> String
where
    S: AsRef<str>,
{
    let invalid_chars = ['<', '>', ':', '\'', '"', '/', '\\', '|', '?', '*'];
    input
        .as_ref()
        .chars()
        .filter(|&c| !invalid_chars.contains(&c) && !c.is_control())
        .collect()
}

const DOT_PATH: &str = ".spotify-dl";

pub(crate) fn get_dot_path() -> Result<PathBuf> {
    let path = dirs::home_dir()
        .map(|p| p.join(DOT_PATH))
        .ok_or(anyhow::anyhow!("Could not find home directory"))?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
