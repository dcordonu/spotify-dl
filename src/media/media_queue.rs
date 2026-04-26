use crate::media::media_item::MediaItem;
use anyhow::Result;
use librespot::core::{Session, SpotifyUri};
use librespot::metadata::Metadata;

/// A collection of [`MediaItem`]s. It is compatible with [`SpotifyUri::Album`],
/// [`SpotifyUri::Playlist`] and [`SpotifyUri::Show`].
pub struct MediaQueue {
    uri: SpotifyUri,
}

impl MediaQueue {
    pub fn from_uri(uri: &SpotifyUri) -> Result<Self> {
        match uri {
            SpotifyUri::Album { .. } | SpotifyUri::Playlist { .. } | SpotifyUri::Show { .. } => {
                Ok(MediaQueue { uri: uri.clone() })
            }
            _ => Err(anyhow::anyhow!("Invalid uri")),
        }
    }

    pub async fn get_tracks(&self, session: &Session) -> Result<Vec<MediaItem>> {
        match &self.uri {
            SpotifyUri::Album { .. } => {
                let album = librespot::metadata::Album::get(session, &self.uri)
                    .await
                    .expect("Failed to get album");
                album
                    .tracks()
                    .filter_map(|uri| match uri {
                        SpotifyUri::Track { .. } => Some(MediaItem::from(uri)),
                        _ => None,
                    })
                    .collect()
            }
            SpotifyUri::Playlist { .. } => {
                let playlist = librespot::metadata::Playlist::get(session, &self.uri)
                    .await
                    .expect("Failed to get playlist");
                playlist
                    .tracks()
                    .filter_map(|uri| match uri {
                        SpotifyUri::Track { .. } => Some(MediaItem::from(uri)),
                        _ => None,
                    })
                    .collect()
            }
            SpotifyUri::Show { .. } => librespot::metadata::Show::get(session, &self.uri)
                .await
                .expect("Failed to get show")
                .episodes
                .iter()
                .filter_map(|uri| match uri {
                    SpotifyUri::Episode { .. } => Some(MediaItem::from(uri)),
                    _ => None,
                })
                .collect(),
            _ => Err(anyhow::anyhow!("Invalid uri")), // this will never be reached
        }
    }
}
