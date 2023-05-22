use std::{str::FromStr, string::FromUtf8Error};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub trait Paginated {
    fn offset(&self) -> usize;
    fn limit(&self) -> usize;
    fn total(&self) -> usize;
}

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
pub struct ArtistSearchPage {
    pub limit: i64,
    pub offset: i64,
    pub total_number_of_items: i64,
    pub items: Vec<Item>,
}

impl Paginated for ArtistSearchPage {
    fn offset(&self) -> usize {
        self.offset as usize
    }
    fn limit(&self) -> usize {
        self.limit as usize
    }
    fn total(&self) -> usize {
        self.total_number_of_items as usize
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub artist_types: Vec<String>,
    pub url: String,
    pub picture: Value,
    pub popularity: i64,
    pub artist_roles: Vec<ArtistRole>,
    pub mixes: Mixes,
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
    pub user_id: String,
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

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub duration: u64,
    pub replay_gain: f64,
    pub peak: f64,
    pub allow_streaming: bool,
    pub stream_ready: bool,
    pub stream_start_date: Option<String>,
    pub premium_streaming_only: bool,
    pub track_number: u64,
    pub volume_number: u64,
    pub version: Value,
    pub popularity: u64,
    pub copyright: Option<String>,
    pub url: Option<String>,
    pub isrc: Option<String>,
    pub editable: bool,
    pub explicit: bool,
    pub audio_quality: String,
    pub audio_modes: Vec<String>,
    pub artist: Artist,
    pub artists: Vec<Artist>,
    pub album: Album,
    pub mixes: Mixes,
}

impl From<Track> for crabidy_core::proto::crabidy::Track {
    fn from(track: Track) -> Self {
        Self {
            uuid: track.id.to_string(),
            title: track.title,
            artist: track.artist.name,
            duration: Some(track.duration as u32),
        }
    }
}

impl From<&Track> for crabidy_core::proto::crabidy::Track {
    fn from(track: &Track) -> Self {
        Self {
            uuid: track.id.to_string(),
            title: track.title.clone(),
            artist: track.artist.name.clone(),
            duration: Some(track.duration as u32),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub picture: Value,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist2 {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub picture: Value,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub cover: String,
    pub vibrant_color: String,
    pub video_cover: Value,
    pub release_date: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mixes {
    #[serde(rename = "TRACK_MIX")]
    pub track_mix: Option<String>,
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
            name: a.title,
            uuid: format!("playlist:{}", a.uuid),
            tracks: Vec::new(),
            parent: None,
            state: crabidy_core::proto::crabidy::LibraryNodeState::Done as i32,
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
pub struct PlaylistTracksPage {
    pub limit: usize,
    pub offset: usize,
    pub total_number_of_items: usize,
    pub items: Vec<PlaylistTrack>,
}

impl Paginated for PlaylistTracksPage {
    fn offset(&self) -> usize {
        self.offset
    }

    fn limit(&self) -> usize {
        self.limit
    }

    fn total(&self) -> usize {
        self.total_number_of_items
    }
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
    pub mixes: Mixes,
    pub date_added: String,
    pub index: i64,
    pub item_uuid: String,
}
