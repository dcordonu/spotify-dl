use crate::media::metadata::MediaItemMetadata;
use anyhow::Result;
use bytes::Bytes;
use librespot::core::{Session, SpotifyUri};
use librespot::metadata::image::Images;
use librespot::metadata::{Artist, Metadata};

/// Represents a single downloadable media item from Spotify.
/// A `MediaItem` can wrap either a [`SpotifyUri::Track`] or a [`SpotifyUri::Episode`],
/// providing a unified interface for interacting with Spotify's playable content.
#[derive(Clone, Debug)]
pub struct MediaItem {
    uri: SpotifyUri,
}

impl MediaItem {
    pub fn from(uri: &SpotifyUri) -> Result<Self> {
        match uri {
            SpotifyUri::Track { .. } | SpotifyUri::Episode { .. } => Ok(Self { uri: uri.clone() }),
            _ => Err(anyhow::anyhow!("Failed to load media item: {:?}", uri)),
        }
    }

    pub fn uri(&self) -> &SpotifyUri {
        &self.uri
    }

    pub fn new(track: &str) -> Result<Self> {
        let uri = crate::utils::parse_uri_or_url(track)
            .map(|uri| match uri {
                SpotifyUri::Album { .. } => Some(uri),
                _ => None,
            })
            .unwrap()
            .ok_or(anyhow::anyhow!("Invalid track"))?;
        Ok(MediaItem { uri })
    }

    pub async fn metadata(&self, session: &Session) -> Result<MediaItemMetadata> {
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

                Ok(MediaItemMetadata::from(
                    metadata,
                    artists,
                    album,
                    get_cover(&covers, &session).await,
                ))
            }
            SpotifyUri::Episode { .. } => {
                let metadata = librespot::metadata::Episode::get(session, &self.uri)
                    .await
                    .map_err(|_| anyhow::anyhow!("Failed to get metadata"))?;

                Ok(MediaItemMetadata::EpisodeMetadata {
                    show_name: metadata.show_name,
                    episode_name: metadata.name,
                    duration: metadata.duration,
                    album_cover: get_cover(&metadata.covers, session).await,
                })
            }
            _ => Err(anyhow::anyhow!("Failed to get metadata")),
        }
    }
}

async fn get_cover(covers: &Images, session: &Session) -> Option<Bytes> {
    match covers.first() {
        Some(c) => session.spclient().get_image(&c.id).await.ok(),
        None => None,
    }
}
