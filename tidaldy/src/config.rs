use serde::{Deserialize, Serialize};
use std::iter::zip;
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    pub base_url: String,
    pub hifi_url: String,
    pub audio_quality: AudioQuality,
    pub login: LoginConfig,
    pub oauth: OauthConfig,
}

impl Default for Settings {
    fn default() -> Self {
        let client_id_key = b"abcdefghijklmnop";
        let hidden_id_from_fulltext_search = "\u{1b}7W<-01\u{3}\nX\u{1f}(=\u{1}[\u{4}".as_bytes();
        let mut client_id_bytes = Vec::new();
        for (k, c) in zip(client_id_key, hidden_id_from_fulltext_search) {
            client_id_bytes.push(*c ^ *k);
        }
        let client_id = String::from_utf8(client_id_bytes).unwrap();

        let client_secret_key = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQR";
        let hidden_secret_from_fulltext_search = "7((\u{c}! \u{16}\"9\u{1b}\u{1d}\u{1f}=8!2'D\u{6}\u{1f}-\"=\u{15}\u{e}\u{16}7 70\u{15}q0$\u{4}&9/z|<5eo".as_bytes();
        let mut client_secret_bytes = Vec::new();
        for (k, c) in zip(client_secret_key, hidden_secret_from_fulltext_search) {
            client_secret_bytes.push(*c ^ *k);
        }
        let client_secret = String::from_utf8(client_secret_bytes).unwrap();

        Self {
            base_url: "https://api.tidal.com/v1".to_string(),
            hifi_url: "https://api.tidalhifi.com/v1".to_string(),
            audio_quality: AudioQuality::Lossless,
            login: LoginConfig {
                device_code: None,
                user_id: None,
                country_code: None,
                access_token: None,
                refresh_token: None,
                expires_after: None,
            },
            oauth: OauthConfig {
                client_id,
                client_secret,
                base_url: "https://auth.tidal.com/v1/oauth2".to_string(),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoginConfig {
    pub device_code: Option<String>,
    pub user_id: Option<String>,
    pub country_code: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_after: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OauthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub base_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AudioQuality {
    Low,
    High,
    Lossless,
    HiRes,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to write config file")]
    Write,
}
