use std::{str::FromStr, string::FromUtf8Error};

use crabidy_core::proto::crabidy::{LibraryNode, LibraryNodeChild};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub limit: Option<usize>,
    pub offset: usize,
    pub total_number_of_items: usize,
    pub items: Vec<T>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistItem {
    pub created: String,
    pub item: Artist,
}

impl From<ArtistItem> for LibraryNode {
    fn from(item: ArtistItem) -> Self {
        Self {
            uuid: format!("artist:{}", item.item.id),
            title: item.item.name,
            children: Vec::new(),
            parent: None,
            tracks: Vec::new(),
            is_queable: true,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub artist_types: Option<Vec<String>>,
    pub url: Option<String>,
    pub picture: Option<Value>,
    pub popularity: Option<i64>,
    pub artist_roles: Option<Vec<ArtistRole>>,
    pub mixes: Option<ArtistMixes>,
}

impl From<Artist> for LibraryNode {
    fn from(artist: Artist) -> Self {
        Self {
            uuid: format!("node:artist:{}", artist.id),
            title: artist.name,
            children: Vec::new(),
            parent: None,
            tracks: Vec::new(),
            is_queable: true,
        }
    }
}

impl From<Artist> for LibraryNodeChild {
    fn from(artist: Artist) -> Self {
        Self {
            uuid: format!("node:artist:{}", artist.id),
            title: artist.name,
            is_queable: true,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistRole {
    pub category_id: i64,
    pub category: String,
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("connecting to the tidal api servers failed")]
    ConnectionError,
    #[error("internal reqwest error")]
    HttpClientError(#[from] reqwest::Error),
    #[error("internal serde url error")]
    SerdeUrlError(#[from] serde_urlencoded::ser::Error),
    #[error("authentication failed")]
    AuthError(String),
    #[error("base64 decoding failed")]
    Base64DecodeError(#[from] base64::DecodeError),
    #[error("utf8 decoding failed")]
    Utf8DecodeError(#[from] FromUtf8Error),
    #[error("json decoding failed")]
    JsonDecodeError(#[from] serde_json::Error),
}

impl From<ClientError> for crabidy_core::ProviderError {
    fn from(err: ClientError) -> Self {
        match err {
            ClientError::ConnectionError => Self::FetchError,
            ClientError::HttpClientError(_) => Self::FetchError,
            ClientError::SerdeUrlError(_) => Self::FetchError,
            _ => Self::Other,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct DeviceAuthRequest {
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_code: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RefreshResponse {
    pub user: UserResponse,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct UserResponse {
    pub user_id: u64,
    pub country_code: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackPlayback {
    pub track_id: i64,
    pub asset_presentation: String,
    pub audio_mode: String,
    pub audio_quality: String,
    pub manifest_mime_type: String,
    pub manifest_hash: String,
    pub manifest: String,
    pub album_replay_gain: f64,
    pub album_peak_amplitude: f64,
    pub track_replay_gain: f64,
    pub track_peak_amplitude: f64,
}

impl TrackPlayback {
    pub fn get_manifest(&self) -> Result<PlaybackManifest, ClientError> {
        PlaybackManifest::from_str(&self.manifest)
    }
}

// #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Track {
//     pub id: u64,
//     pub title: String,
//     pub duration: u64,
//     pub replay_gain: f64,
//     pub peak: f64,
//     pub allow_streaming: bool,
//     pub stream_ready: bool,
//     pub stream_start_date: Option<String>,
//     pub premium_streaming_only: bool,
//     pub track_number: u64,
//     pub volume_number: u64,
//     pub version: Value,
//     pub popularity: u64,
//     pub copyright: Option<String>,
//     pub url: Option<String>,
//     pub isrc: Option<String>,
//     pub editable: bool,
//     pub explicit: bool,
//     pub audio_quality: String,
//     pub audio_modes: Vec<String>,
//     pub artist: Artist,
//     pub artists: Vec<Artist>,
//     pub album: Album,
//     pub mixes: TrackMixes,
// }

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub title: String,
    pub duration: Option<i64>,
    pub replay_gain: Option<f64>,
    pub peak: Option<f64>,
    pub allow_streaming: Option<bool>,
    pub stream_ready: Option<bool>,
    pub ad_supported_stream_ready: Option<bool>,
    pub stream_start_date: Option<String>,
    pub premium_streaming_only: Option<bool>,
    pub track_number: Option<i64>,
    pub volume_number: Option<i64>,
    pub version: Option<Value>,
    pub popularity: Option<i64>,
    pub copyright: Option<String>,
    pub url: Option<String>,
    pub isrc: Option<String>,
    pub editable: Option<bool>,
    pub explicit: Option<bool>,
    pub audio_quality: Option<String>,
    pub audio_modes: Option<Vec<String>>,
    pub media_metadata: Option<MediaMetadata>,
    pub artist: Option<Artist>,
    pub artists: Option<Vec<Artist>>,
    pub album: Option<Album>,
    pub mixes: Option<TrackMixes>,
}
impl From<Track> for crabidy_core::proto::crabidy::Track {
    fn from(track: Track) -> Self {
        Self {
            uuid: format!("track:{}", track.id),
            title: track.title,
            artist: match track.artist {
                Some(a) => a.name.clone(),
                None => "".to_string(),
            },
            album: track.album.map(|a| a.into()),
            duration: track.duration.map(|d| d as u32 * 1000),
        }
    }
}

impl From<&Track> for crabidy_core::proto::crabidy::Track {
    fn from(track: &Track) -> Self {
        Self {
            uuid: format!("track:{}", track.id),
            title: track.title.clone(),
            artist: match track.artist.as_ref() {
                Some(a) => a.name.clone(),
                None => "".to_string(),
            },
            album: track.album.clone().map(|a| a.into()),
            duration: track.duration.map(|d| d as u32 * 1000),
        }
    }
}

// #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Artist {
//     pub id: i64,
//     pub name: String,
//     #[serde(rename = "type")]
//     pub type_field: String,
//     pub picture: Value,
// }

// #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Artist2 {
//     pub id: i64,
//     pub name: String,
//     #[serde(rename = "type")]
//     pub type_field: String,
//     pub picture: Value,
// }

// #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Album {
//     pub id: i64,
//     pub title: String,
//     pub cover: String,
//     pub vibrant_color: String,
//     pub video_cover: Value,
//     pub release_date: Option<String>,
// }
//
// #[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct Root {
//     pub id: i64,
//     pub title: String,
//     pub duration: i64,
//     pub stream_ready: bool,
//     pub ad_supported_stream_ready: bool,
//     pub stream_start_date: String,
//     pub allow_streaming: bool,
//     pub premium_streaming_only: bool,
//     pub number_of_tracks: i64,
//     pub number_of_videos: i64,
//     pub number_of_volumes: i64,
//     pub release_date: String,
//     pub copyright: String,
//     #[serde(rename = "type")]
//     pub type_field: String,
//     pub version: Value,
//     pub url: String,
//     pub cover: String,
//     pub vibrant_color: String,
//     pub video_cover: Value,
//     pub explicit: bool,
//     pub upc: String,
//     pub popularity: i64,
//     pub audio_quality: String,
//     pub audio_modes: Vec<String>,
//     pub media_metadata: MediaMetadata,
//     pub artist: Artist,
//     pub artists: Vec<Artist2>,
// }
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub cover: Option<String>,
    pub vibrant_color: Option<String>,
    pub release_date: Option<String>,
    pub duration: Option<i64>,
    pub stream_ready: Option<bool>,
    pub ad_supported_stream_ready: Option<bool>,
    pub stream_start_date: Option<String>,
    pub allow_streaming: Option<bool>,
    pub premium_streaming_only: Option<bool>,
    pub number_of_tracks: Option<i64>,
    pub number_of_videos: Option<i64>,
    pub number_of_volumes: Option<i64>,
    pub copyright: Option<String>,
    #[serde(rename = "type")]
    pub type_field: Option<String>,
    pub version: Option<Value>,
    pub url: Option<String>,
    pub video_cover: Option<Value>,
    pub explicit: Option<bool>,
    pub upc: Option<String>,
    pub popularity: Option<i64>,
    pub audio_quality: Option<String>,
    pub audio_modes: Option<Vec<String>>,
    pub media_metadata: Option<MediaMetadata>,
    pub artist: Option<Artist>,
    pub artists: Option<Vec<Artist>>,
}

impl From<Album> for crabidy_core::proto::crabidy::LibraryNode {
    fn from(album: Album) -> Self {
        Self {
            uuid: format!("node:album:{}", album.id),
            title: album.title,
            children: Vec::new(),
            parent: None,
            tracks: Vec::new(),
            is_queable: true,
        }
    }
}

impl From<Album> for crabidy_core::proto::crabidy::LibraryNodeChild {
    fn from(album: Album) -> Self {
        Self {
            uuid: format!("node:album:{}", album.id),
            title: album.title,
            is_queable: true,
        }
    }
}

impl From<&Album> for crabidy_core::proto::crabidy::LibraryNodeChild {
    fn from(album: &Album) -> Self {
        Self {
            uuid: format!("node:album:{}", album.id),
            title: album.title.clone(),
            is_queable: true,
        }
    }
}

impl From<Album> for crabidy_core::proto::crabidy::Album {
    fn from(album: Album) -> Self {
        Self {
            title: album.title,
            release_date: album.release_date,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub tags: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMixes {
    #[serde(rename = "TRACK_MIX")]
    pub track_mix: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistMixes {
    #[serde(rename = "MASTER_ARTIST_MIX")]
    pub master_artist_mix: Option<String>,
    #[serde(rename = "ARTIST_MIX")]
    pub artist_mix: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct PlaybackManifest {
    pub mime_type: String,
    pub codecs: String,
    pub encryption_type: EncryptionType,
    pub key_id: Option<String>,
    pub urls: Vec<String>,
}

impl FromStr for PlaybackManifest {
    type Err = ClientError;
    fn from_str(input: &str) -> Result<PlaybackManifest, Self::Err> {
        let decode = base64::decode(input)?;
        let json = String::from_utf8(decode)?;
        let parsed: PlaybackManifest = serde_json::from_str(&json)?;
        Ok(parsed)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum EncryptionType {
    #[serde(rename = "NONE")]
    None,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub username: String,
    pub profile_name: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub email_verified: bool,
    pub country_code: String,
    pub created: String,
    pub newsletter: bool,
    #[serde(rename = "acceptedEULA")]
    pub accepted_eula: bool,
    pub gender: Value,
    pub date_of_birth: String,
    pub facebook_uid: i64,
    pub apple_uid: Value,
    pub partner: i64,
    pub tidal_id: Value,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistAndFavorite {
    #[serde(rename = "type")]
    pub type_field: String,
    pub created: String,
    pub playlist: Playlist,
}

impl From<PlaylistAndFavorite> for crabidy_core::proto::crabidy::LibraryNode {
    fn from(a: PlaylistAndFavorite) -> Self {
        a.playlist.into()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub uuid: String,
    pub title: String,
    pub number_of_tracks: Option<usize>,
    pub number_of_videos: Option<usize>,
    pub creator: Option<Creator>,
    pub description: Option<String>,
    pub duration: Option<u32>,
    pub last_updated: Option<String>,
    pub created: Option<String>,
    #[serde(rename = "type")]
    pub type_field: Option<String>,
    pub public_playlist: bool,
    pub url: Option<String>,
    pub image: Option<String>,
    pub popularity: Option<i64>,
    pub square_image: Option<String>,
    pub promoted_artists: Option<Vec<Value>>,
    pub last_item_added_at: Option<String>,
}

impl From<Playlist> for crabidy_core::proto::crabidy::LibraryNode {
    fn from(a: Playlist) -> Self {
        crabidy_core::proto::crabidy::LibraryNode {
            title: a.title,
            uuid: format!("node:playlist:{}", a.uuid),
            tracks: Vec::new(),
            parent: None,
            children: Vec::new(),
            is_queable: true,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Creator {
    pub id: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistTrack {
    pub id: i64,
    pub title: String,
    pub duration: i64,
    pub replay_gain: f64,
    pub peak: f64,
    pub allow_streaming: bool,
    pub stream_ready: bool,
    pub stream_start_date: Option<String>,
    pub premium_streaming_only: bool,
    pub track_number: i64,
    pub volume_number: i64,
    pub version: Option<String>,
    pub popularity: i64,
    pub copyright: String,
    pub description: Value,
    pub url: String,
    pub isrc: String,
    pub editable: bool,
    pub explicit: bool,
    pub audio_quality: String,
    pub audio_modes: Vec<String>,
    pub artist: Artist,
    pub artists: Vec<Artist>,
    pub album: Album,
    pub mixes: TrackMixes,
    pub date_added: String,
    pub index: i64,
    pub item_uuid: String,
}
