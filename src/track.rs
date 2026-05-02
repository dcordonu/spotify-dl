use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use lazy_static::lazy_static;
use librespot::core::session::Session;
use librespot::core::{SpotifyId, SpotifyUri};
use librespot::metadata::{Artist, Metadata};
use librespot::metadata::image::{Image, Images};
use regex::Regex;

use crate::encoder::tags::Tags;
use crate::utils::clean_invalid_characters;

pub type AsyncFn<T> =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<T>> + Send>> + Send + Sync>;

#[async_trait::async_trait]
trait TrackCollection {
    async fn get_tracks(&self, session: &Session) -> Vec<Track>;
}

#[tracing::instrument(name = "get_tracks", skip(session), level = "debug")]
pub async fn get_tracks(spotify_ids: Vec<String>, session: &Session) -> Result<Vec<Track>> {
    let mut tracks: Vec<Track> = Vec::new();
    for id in spotify_ids {
        tracing::debug!("Getting tracks for: {}", id);
        let uri: SpotifyUri = parse_uri_or_url(&id).ok_or(anyhow::anyhow!("Invalid track"))?;
        let new_tracks = match uri {
            SpotifyUri::Track { .. } | SpotifyUri::Episode { .. } => vec![Track { uri: uri.clone() }],
            SpotifyUri::Album { id } => Album::from_id(id).get_tracks(session).await,
            SpotifyUri::Playlist { id, .. } => Playlist::from_id(id).get_tracks(session).await,
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

fn parse_uri_or_url(track: &str) -> Option<SpotifyUri> {
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

#[derive(Clone, Debug)]
pub struct Track {
    pub uri: SpotifyUri
}

lazy_static! {
    static ref SPOTIFY_URL_REGEX: Regex =
        Regex::new(r"https://open\.spotify\.com(?:/intl-[a-z]{2})?/(\w+)/([a-zA-Z0-9]+)").unwrap();
}

impl Track {
    pub fn new(track: &str) -> Result<Self> {
        let uri = parse_uri_or_url(track)
            .map(|uri| match uri {
                SpotifyUri::Album { .. } => Some(uri),
                _ => None
            })
            .unwrap()
            .ok_or(anyhow::anyhow!("Invalid track"))?;
        Ok(Track { uri })
    }

    pub async fn metadata(&self, session: &Session) -> Result<CustomMetadata> {
        match &self.uri {
            SpotifyUri::Track { .. } => {
                let metadata = librespot::metadata::Track::get(session, &self.uri)
                    .await
                    .map_err(|_| anyhow::anyhow!("Failed to get metadata"))?;
                let mut artists = Vec::new();
                for artist in metadata.artists.iter() {
                    artists.push(
                        Artist::get(session, &artist.id)
                            .await
                            .map_err(|_| anyhow::anyhow!("Failed to get artist"))?,
                    );
                }
                let album = librespot::metadata::Album::get(session, &metadata.album.id)
                    .await
                    .map_err(|_| anyhow::anyhow!("Failed to get album"))?;

                let covers = metadata.album.covers.clone();

                Ok(CustomMetadata::from(
                    metadata,
                    artists,
                    album,
                    get_cover(&covers, &session).await,
                ))
            },
            SpotifyUri::Episode { .. } => {
                let metadata = librespot::metadata::Episode::get(session, &self.uri)
                    .await
                    .map_err(|_| anyhow::anyhow!("Failed to get metadata"))?;

                Ok(CustomMetadata::CustomEpisodeMetadata {
                    show_name: metadata.show_name,
                    episode_name: metadata.name,
                    duration: metadata.duration,
                    album_cover: get_cover(&metadata.covers, session).await,
                })
            }
            _ => Err(anyhow::anyhow!("Failed to get metadata"))
        }
    }
}

async fn get_cover(covers: &Images, session: &Session) -> Option<Bytes> {
    match covers.first() {
        Some(c) => session.spclient().get_image(&c.id).await.ok(),
        None => None
    }
}

#[async_trait::async_trait]
impl TrackCollection for Track {
    async fn get_tracks(&self, _session: &Session) -> Vec<Track> {
        vec![self.clone()]
    }
}

pub struct Album {
    id: SpotifyId,
}

impl Album {
    pub fn new(album: &str) -> Result<Self> {
        let id = parse_uri_or_url(album)
            .map(|uri| match uri {
                SpotifyUri::Album { id } => Some(id),
                _ => None
            })
            .unwrap()
            .ok_or(anyhow::anyhow!("Invalid album"))?;
        Ok(Album { id })
    }

    pub fn from_id(id: SpotifyId) -> Self {
        Album { id }
    }

    pub async fn is_album(id: SpotifyUri, session: &Session) -> bool {
        librespot::metadata::Album::get(session, &id).await.is_ok()
    }
}

#[async_trait::async_trait]
impl TrackCollection for Album {
    async fn get_tracks(&self, session: &Session) -> Vec<Track> {
        let album = librespot::metadata::Album::get(session, &SpotifyUri::Album { id: self.id })
            .await
            .expect("Failed to get album");
        album.tracks()
            .filter_map(|uri| match uri {
                SpotifyUri::Album { .. } => Some(Track { uri: uri.clone() }),
                _ => None
            })
            .collect()
    }
}

pub struct Playlist {
    uri: SpotifyUri,
}

impl Playlist {
    pub fn new(playlist: &str) -> Result<Self> {
        let id = parse_uri_or_url(playlist).ok_or(anyhow::anyhow!("Invalid playlist"))?;
        Ok(Playlist { uri: id })
    }

    pub fn from_id(id: SpotifyId) -> Self {
        Playlist { uri: SpotifyUri::Playlist { user: None, id } }
    }

    pub async fn is_playlist(id: SpotifyUri, session: &Session) -> bool {
        librespot::metadata::Playlist::get(session, &id)
            .await
            .is_ok()
    }
}

#[async_trait::async_trait]
impl TrackCollection for Playlist {
    async fn get_tracks(&self, session: &Session) -> Vec<Track> {
        let playlist = librespot::metadata::Playlist::get(session, &self.uri)
            .await
            .expect("Failed to get playlist");
        playlist
            .tracks()
            .filter_map(|uri| match uri {
                SpotifyUri::Track { .. } => Some(Track { uri: uri.clone() }),
                _ => None
            })
            .collect()
    }
}

#[derive(Clone)]
pub enum CustomMetadata {
    CustomEpisodeMetadata {
        duration: i32,
        episode_name: String,
        show_name: String,
        album_cover: Option<Bytes>,
    },
    CustomTrackMetadata {
        artists: Vec<ArtistMetadata>,
        track_name: String,
        album: AlbumMetadata,
        duration: i32,
        album_cover: Option<Bytes>,
    }
}

impl CustomMetadata {
    pub fn track_name(&self) -> &String {
        match &self {
            CustomMetadata::CustomEpisodeMetadata { episode_name, .. } => episode_name,
            CustomMetadata::CustomTrackMetadata { track_name, .. } => track_name,
        }
    }

    pub fn duration(&self) -> usize {
        let duration = match &self {
            CustomMetadata::CustomEpisodeMetadata { duration, .. } => *duration,
            CustomMetadata::CustomTrackMetadata { duration, .. } => *duration,
        };

        duration as usize
    }

    pub fn from(
        track: librespot::metadata::Track,
        artists: Vec<Artist>,
        album: librespot::metadata::Album,
        album_cover: Option<Bytes>
    ) -> Self {
        let artists = artists
            .iter()
            .map(|artist| ArtistMetadata::from(artist.clone()))
            .collect();
        let album = AlbumMetadata::from(album);

        CustomMetadata::CustomTrackMetadata {
            artists,
            track_name: track.name.clone(),
            album,
            duration: track.duration,
            album_cover,
        }
    }

    pub fn approx_size(&self) -> usize {
        let duration = self.duration() / 1000;
        let sample_rate = 44100;
        let channels = 2;
        let bits_per_sample = 32;
        let bytes_per_sample = bits_per_sample / 8;
        duration * sample_rate * channels * bytes_per_sample
    }

    pub async fn tags(&self) -> Result<Tags> {
        let tags = match &self {
            CustomMetadata::CustomEpisodeMetadata { episode_name, album_cover, show_name, ..} => {
                Tags {
                    title: episode_name.clone(),
                    album_cover: album_cover.clone(),
                    album_title: show_name.clone(),
                    artists: vec![]
                }
            },
            CustomMetadata::CustomTrackMetadata { track_name, artists, album, album_cover, .. } => {
                Tags {
                    title: track_name.clone(),
                    artists: artists.iter().map(|a| a.name.clone()).collect(),
                    album_title: album.name.clone(),
                    album_cover: album_cover.clone(),
                }
            }
        };
        Ok(tags)
    }
}

impl ToString for CustomMetadata {
    fn to_string(&self) -> String {
        match &self {
            CustomMetadata::CustomTrackMetadata { artists, track_name, .. } => {
                if artists.len() > 3 {
                    let artists_name = artists
                        .iter()
                        .take(3)
                        .map(|artist| artist.name.clone())
                        .collect::<Vec<String>>()
                        .join(", ");
                    return clean_invalid_characters(format!(
                        "{}, ... - {}",
                        artists_name, track_name
                    ));
                }

                let artists_name = artists
                    .iter()
                    .map(|artist| artist.name.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                clean_invalid_characters(format!("{} - {}", artists_name, track_name))
            },
            CustomMetadata::CustomEpisodeMetadata { episode_name, show_name, .. } => {
                clean_invalid_characters(format!("{} - {}", show_name, episode_name))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArtistMetadata {
    pub name: String,
}

impl From<librespot::metadata::Artist> for ArtistMetadata {
    fn from(artist: librespot::metadata::Artist) -> Self {
        ArtistMetadata {
            name: artist.name.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AlbumMetadata {
    pub name: String,
    pub cover: Option<Image>,
}

impl From<librespot::metadata::Album> for AlbumMetadata {
    fn from(album: librespot::metadata::Album) -> Self {
        AlbumMetadata {
            name: album.name.clone(),
            cover: album.covers.first().cloned(),
        }
    }
}
