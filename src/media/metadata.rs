use crate::encoder::tags::Tags;
#[cfg(doc)]
use crate::media::media_item::MediaItem;
use crate::utils::clean_invalid_characters;
use anyhow::Result;
use bytes::Bytes;
#[cfg(doc)]
use librespot::core::SpotifyUri;
use librespot::metadata::Artist;

/// Holds the metadata extracted for each supported type of [`SpotifyUri`] on [`MediaItem`].
#[derive(Clone)]
pub enum MediaItemMetadata {
    EpisodeMetadata {
        duration: i32,
        episode_name: String,
        show_name: String,
        album_cover: Option<Bytes>,
    },
    TrackMetadata {
        artists: Vec<ArtistMetadata>,
        track_name: String,
        album: AlbumMetadata,
        duration: i32,
        album_cover: Option<Bytes>,
    },
}

#[derive(Clone, Debug)]
pub struct ArtistMetadata {
    name: String,
}

impl From<Artist> for ArtistMetadata {
    fn from(artist: Artist) -> Self {
        ArtistMetadata {
            name: artist.name.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AlbumMetadata {
    name: String,
}

impl From<librespot::metadata::Album> for AlbumMetadata {
    fn from(album: librespot::metadata::Album) -> Self {
        AlbumMetadata {
            name: album.name.clone(),
        }
    }
}

impl MediaItemMetadata {
    pub fn track_name(&self) -> &String {
        match &self {
            MediaItemMetadata::EpisodeMetadata { episode_name, .. } => episode_name,
            MediaItemMetadata::TrackMetadata { track_name, .. } => track_name,
        }
    }

    pub fn duration(&self) -> usize {
        let duration = match &self {
            MediaItemMetadata::EpisodeMetadata { duration, .. } => *duration,
            MediaItemMetadata::TrackMetadata { duration, .. } => *duration,
        };

        duration as usize
    }

    pub fn from(
        track: librespot::metadata::Track,
        artists: Vec<Artist>,
        album: librespot::metadata::Album,
        album_cover: Option<Bytes>,
    ) -> Self {
        let artists = artists
            .iter()
            .map(|artist| ArtistMetadata::from(artist.clone()))
            .collect();
        let album = AlbumMetadata::from(album);

        MediaItemMetadata::TrackMetadata {
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
            MediaItemMetadata::EpisodeMetadata {
                episode_name,
                album_cover,
                show_name,
                ..
            } => Tags {
                title: episode_name.clone(),
                album_cover: album_cover.clone(),
                album_title: show_name.clone(),
                artists: vec![],
            },
            MediaItemMetadata::TrackMetadata {
                track_name,
                artists,
                album,
                album_cover,
                ..
            } => Tags {
                title: track_name.clone(),
                artists: artists.iter().map(|a| a.name.clone()).collect(),
                album_title: album.name.clone(),
                album_cover: album_cover.clone(),
            },
        };
        Ok(tags)
    }
}

impl ToString for MediaItemMetadata {
    fn to_string(&self) -> String {
        match &self {
            MediaItemMetadata::TrackMetadata {
                artists,
                track_name,
                ..
            } => {
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
            }
            MediaItemMetadata::EpisodeMetadata {
                episode_name,
                show_name,
                ..
            } => clean_invalid_characters(format!("{} - {}", show_name, episode_name)),
        }
    }
}
