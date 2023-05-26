/// Lots of stuff and especially the auth handling is shamelessly copied from
/// https://github.com/MinisculeGirraffe/tdl
use reqwest::Client as HttpClient;
use serde::de::DeserializeOwned;
use tokio::time::{sleep, Duration, Instant};
pub mod config;
pub mod models;
use async_trait::async_trait;
pub use models::*;

#[derive(Debug)]
pub struct Client {
    http_client: HttpClient,
    settings: config::Settings,
}

#[async_trait]
impl crabidy_core::ProviderClient for Client {
    async fn init(raw_toml_settings: &str) -> Result<Self, crabidy_core::ProviderError> {
        let settings: config::Settings = if let Ok(settings) = toml::from_str(raw_toml_settings) {
            settings
        } else {
            let settings = config::Settings::default();
            println!(
                "could not parse toml settings: {:#?} using default settings instead: {:#?}",
                raw_toml_settings, settings
            );
            settings
        };

        let mut client = Self::new(settings)?;
        if let Ok(_) = client.login_config().await {
            return Ok(client);
        }
        if let Ok(_) = client.login_web().await {
            return Ok(client);
        }
        Err(crabidy_core::ProviderError::CouldNotLogin)
    }
    fn settings(&self) -> String {
        toml::to_string_pretty(&self.settings).unwrap()
    }
    async fn get_urls_for_track(
        &self,
        track_uuid: &str,
    ) -> Result<Vec<String>, crabidy_core::ProviderError> {
        let Ok(playback) = self.get_track_playback(track_uuid).await else {
                  return Err(crabidy_core::ProviderError::FetchError)
                };
        let Ok(manifest) = playback.get_manifest() else {
                  return Err(crabidy_core::ProviderError::FetchError)
                };
        Ok(manifest.urls)
    }

    async fn get_metadata_for_track(
        &self,
        track_uuid: &str,
    ) -> Result<crabidy_core::proto::crabidy::Track, crabidy_core::ProviderError> {
        let Ok(track) = self.get_track(track_uuid).await else {
                  return Err(crabidy_core::ProviderError::FetchError)
                };
        Ok(track.into())
    }

    fn get_lib_root(&self) -> crabidy_core::proto::crabidy::LibraryNode {
        let global_root = crabidy_core::proto::crabidy::LibraryNode::new();
        let children = vec![crabidy_core::proto::crabidy::LibraryNodeChild::new(
            "userplaylists".to_string(),
            "playlists".to_string(),
        )];
        crabidy_core::proto::crabidy::LibraryNode {
            uuid: "tidal".to_string(),
            title: "tidal".to_string(),
            parent: Some(format!("{}", global_root.uuid)),
            state: crabidy_core::proto::crabidy::LibraryNodeState::Done as i32,
            tracks: Vec::new(),
            children,
            is_queable: false,
        }
    }

    async fn get_lib_node(
        &self,
        uuid: &str,
    ) -> Result<crabidy_core::proto::crabidy::LibraryNode, crabidy_core::ProviderError> {
        let Some(user_id) = self.settings.login.user_id.clone() else {
          return Err(crabidy_core::ProviderError::UnknownUser)
    };
        let (module, uuid) = split_uuid(uuid);
        let node = match module.as_str() {
            "userplaylists" => {
                let mut node = crabidy_core::proto::crabidy::LibraryNode {
                    uuid: "userplaylists".to_string(),
                    title: "playlists".to_string(),
                    parent: Some("tidal".to_string()),
                    state: crabidy_core::proto::crabidy::LibraryNodeState::Unspecified as i32,
                    tracks: Vec::new(),
                    children: Vec::new(),
                    is_queable: false,
                };
                for playlist in self
                    .get_users_playlists_and_favorite_playlists(&user_id)
                    .await?
                {
                    let child = crabidy_core::proto::crabidy::LibraryNodeChild::new(
                        format!("playlist:{}", playlist.playlist.uuid),
                        playlist.playlist.title,
                    );
                    node.children.push(child);
                }
                node
            }
            "playlist" => {
                let mut node: crabidy_core::proto::crabidy::LibraryNode =
                    self.get_playlist(&uuid).await?.into();
                let tracks: Vec<crabidy_core::proto::crabidy::Track> = self
                    .get_playlist_tracks(&uuid)
                    .await?
                    .iter()
                    .map(|t| t.into())
                    .collect();
                node.tracks = tracks;
                node.parent = Some("userplaylists".to_string());
                node
            }
            _ => return Err(crabidy_core::ProviderError::MalformedUuid),
        };
        Ok(node)
    }
}

fn split_uuid(uuid: &str) -> (String, String) {
    let mut split = uuid.splitn(2, ':');
    (
        split.next().unwrap_or("").to_string(),
        split.next().unwrap_or("").to_string(),
    )
}

impl Client {
    pub fn new(settings: config::Settings) -> Result<Self, ClientError> {
        let http_client = HttpClient::builder()
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36 Edg/91.0.864.59")
    .build()?;

        Ok(Self {
            http_client,
            settings,
        })
    }

    pub fn get_user_id(&self) -> Option<String> {
        self.settings.login.user_id.clone()
    }

    pub async fn make_request<T: DeserializeOwned>(
        &self,
        uri: &str,
        query: Option<&[(&str, String)]>,
    ) -> Result<T, ClientError> {
        let Some(ref access_token) = self.settings.login.access_token.clone() else {
            return Err(ClientError::AuthError(
                "No access token found".to_string(),
            ))
        };
        let Some(country_code) = self.settings.login.country_code.clone() else {
            return Err(ClientError::AuthError(
                "No country code found".to_string(),
            ))
        };
        let country_param = ("countryCode", country_code);
        let mut params: Vec<&(&str, String)> = vec![&country_param];
        if let Some(query) = query {
            params.extend(query);
        }

        let response: T = self
            .http_client
            .get(format!("{}/{}", self.settings.hifi_url, uri))
            .bearer_auth(access_token)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        Ok(response)
    }

    pub async fn make_paginated_request<T: DeserializeOwned>(
        &self,
        uri: &str,
        query: Option<&[(&str, String)]>,
    ) -> Result<Vec<T>, ClientError> {
        let Some(ref access_token) = self.settings.login.access_token.clone() else {
            return Err(ClientError::AuthError(
                "No access token found".to_string(),
            ))
        };
        let Some(country_code) = self.settings.login.country_code.clone() else {
            return Err(ClientError::AuthError(
                "No country code found".to_string(),
            ))
        };
        let country_param = ("countryCode", country_code);
        let limit = 50;
        let mut offset = 0;
        let limit_param = ("limit", limit.to_string());
        let mut params: Vec<&(&str, String)> = vec![&country_param, &limit_param];
        if let Some(query) = query {
            params.extend(query);
        }

        let mut response: Page<T> = self
            .http_client
            .get(format!("{}/{}", self.settings.hifi_url, uri))
            .bearer_auth(access_token)
            .query(&params)
            .send()
            .await?
            .json()
            .await?;
        let mut items = Vec::with_capacity(response.total_number_of_items);
        items.extend(response.items);
        while response.offset + limit < response.total_number_of_items {
            offset += limit;
            let offset_param = ("offset", offset.to_string());
            let mut params: Vec<&(&str, String)> =
                vec![&country_param, &limit_param, &offset_param];
            if let Some(query) = query {
                params.extend(query);
            }
            response = self
                .http_client
                .get(format!("{}/{}", self.settings.hifi_url, uri))
                .bearer_auth(access_token)
                .query(&params)
                .send()
                .await?
                .json()
                .await?;
            items.extend(response.items);
        }
        Ok(items)
    }

    pub async fn make_explorer_request(
        &self,
        uri: &str,
        query: Option<&[(&str, String)]>,
    ) -> Result<(), ClientError> {
        let Some(ref access_token) = self.settings.login.access_token.clone() else {
            return Err(ClientError::AuthError(
                "No access token found".to_string(),
            ))
        };
        let Some(country_code) = self.settings.login.country_code.clone() else {
            return Err(ClientError::AuthError(
                "No country code found".to_string(),
            ))
        };
        let country_param = ("countryCode", country_code);
        let mut params: Vec<&(&str, String)> = vec![&country_param];
        if let Some(query) = query {
            params.extend(query);
        }

        let response = self
            .http_client
            .get(format!("{}/{}", self.settings.hifi_url, uri))
            .bearer_auth(access_token)
            .query(&params)
            .send()
            .await?
            .text()
            .await?;
        println!("{:?}", response);
        Ok(())
    }

    pub async fn search(&self, query: &str) -> Result<(), ClientError> {
        let query = vec![("query", query.to_string())];
        self.make_explorer_request(&format!("search/artists"), Some(&query))
            .await?;
        Ok(())
    }

    pub async fn get_playlist_tracks(
        &self,
        playlist_uuid: &str,
    ) -> Result<Vec<Track>, ClientError> {
        Ok(self
            .make_paginated_request(&format!("playlists/{}/tracks", playlist_uuid), None)
            .await?)
    }

    pub async fn get_playlist(&self, playlist_uuid: &str) -> Result<Playlist, ClientError> {
        Ok(self
            .make_request(&format!("playlists/{}", playlist_uuid), None)
            .await?)
    }

    pub async fn get_users_playlists(&self, user_id: u64) -> Result<Vec<Playlist>, ClientError> {
        Ok(self
            .make_paginated_request(&format!("users/{}/playlists", user_id), None)
            .await?)
    }

    pub async fn get_users_playlists_and_favorite_playlists(
        &self,
        user_id: &str,
    ) -> Result<Vec<PlaylistAndFavorite>, ClientError> {
        Ok(self
            .make_paginated_request(
                &format!("users/{}/playlistsAndFavoritePlaylists", user_id),
                None,
            )
            .await?)
    }

    pub async fn explore_get_users_playlists_and_favorite_playlists(
        &self,
        user_id: u64,
    ) -> Result<(), ClientError> {
        let limit = 50;
        let offset = 0;
        let limit_param = ("limit", limit.to_string());
        let offset_param = ("offset", offset.to_string());
        let params: Vec<(&str, String)> = vec![limit_param, offset_param];
        self.make_explorer_request(
            &format!("users/{}/playlistsAndFavoritePlaylists", user_id),
            Some(&params[..]),
        )
        .await?;
        Ok(())
    }

    pub async fn get_users_favorites(&self, user_id: u64) -> Result<(), ClientError> {
        self.make_explorer_request(
            &format!("users/{}/favorites", user_id),
            None,
            // Some(&query),
        )
        .await?;
        Ok(())
    }

    pub async fn get_user(&self, user_id: u64) -> Result<(), ClientError> {
        self.make_explorer_request(
            &format!("users/{}", user_id),
            None,
            // Some(&query),
        )
        .await?;
        Ok(())
    }

    pub async fn get_track_playback(&self, track_id: &str) -> Result<TrackPlayback, ClientError> {
        let query = vec![
            ("audioquality", "LOSSLESS".to_string()),
            ("playbackmode", "STREAM".to_string()),
            ("assetpresentation", "FULL".to_string()),
        ];
        self.make_request(
            &format!("tracks/{}/playbackinfopostpaywall", track_id),
            Some(&query),
        )
        .await
    }

    pub async fn get_track(&self, track_id: &str) -> Result<Track, ClientError> {
        self.make_request(&format!("tracks/{}", track_id), None)
            .await
    }

    pub async fn login_web(&mut self) -> Result<(), ClientError> {
        let code_response = self.get_device_code().await?;
        let now = Instant::now();
        println!("https://{}", code_response.verification_uri_complete);
        while now.elapsed().as_secs() <= code_response.expires_in {
            let login = self.check_auth_status(&code_response.device_code).await;
            if login.is_err() {
                sleep(Duration::from_secs(code_response.interval)).await;
                continue;
            }
            let timestamp = chrono::Utc::now().timestamp() as u64;

            let login_results = login?;
            self.settings.login.device_code = Some(code_response.device_code);
            self.settings.login.access_token = Some(login_results.access_token);
            self.settings.login.refresh_token = login_results.refresh_token;
            self.settings.login.expires_after = Some(login_results.expires_in + timestamp);
            self.settings.login.user_id = Some(login_results.user.user_id.to_string());
            self.settings.login.country_code = Some(login_results.user.country_code);
            return Ok(());
        }
        println!("login attempt expired");
        Err(ClientError::ConnectionError)
    }

    pub async fn login_config(&mut self) -> Result<(), ClientError> {
        let Some(access_token) = self.settings.login.access_token.clone() else {
            return Err(ClientError::AuthError(
                "No access token found".to_string(),
            ))
        };
        //return if our session is still valid
        if self
            .http_client
            .get(format!("{}/sessions", self.settings.base_url))
            .bearer_auth(access_token)
            .send()
            .await?
            .status()
            .is_success()
        {
            return Ok(());
        }

        //otherwise refresh our token
        let refresh = self.refresh_access_token().await?;
        let now = chrono::Utc::now().timestamp() as u64;

        self.settings.login.expires_after = Some(refresh.expires_in + now);
        self.settings.login.access_token = Some(refresh.access_token);
        Ok(())
    }

    pub async fn refresh_access_token(&self) -> Result<RefreshResponse, ClientError> {
        let Some(refresh_token) = self.settings.login.refresh_token.clone() else {
        return Err(ClientError::AuthError(
            "No refresh token found".to_string(),
        ))
      };
        let data = DeviceAuthRequest {
            client_id: self.settings.oauth.client_id.clone(),
            client_secret: Some(self.settings.oauth.client_secret.clone()),
            refresh_token: Some(refresh_token.to_string()),
            grant_type: Some("refresh_token".to_string()),
            ..Default::default()
        };
        let body = serde_urlencoded::to_string(&data)?;

        let req = self
            .http_client
            .post("https://auth.tidal.com/v1/oauth2/token")
            .body(body)
            .basic_auth(
                self.settings.oauth.client_id.clone(),
                Some(self.settings.oauth.client_secret.clone()),
            )
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;
        if req.status().is_success() {
            let res = req.json::<RefreshResponse>().await?;
            Ok(res)
        } else {
            Err(ClientError::AuthError(
                "Failed to refresh access token".to_string(),
            ))
        }
    }
    async fn get_device_code(&self) -> Result<DeviceAuthResponse, ClientError> {
        let req = DeviceAuthRequest {
            client_id: self.settings.oauth.client_id.clone(),
            scope: Some("r_usr+w_usr+w_sub".to_string()),
            ..Default::default()
        };
        let payload = serde_urlencoded::to_string(&req)?;
        let res = self
            .http_client
            .post(format!(
                "{}/device_authorization",
                &self.settings.oauth.base_url
            ))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(payload)
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(ClientError::AuthError(res.status().to_string()));
        }
        let code: DeviceAuthResponse = res.json().await?;
        Ok(code)
    }

    pub async fn check_auth_status(
        &self,
        device_code: &str,
    ) -> Result<RefreshResponse, ClientError> {
        let req = DeviceAuthRequest {
            client_id: self.settings.oauth.client_id.clone(),
            device_code: Some(device_code.to_string()),
            scope: Some("r_usr+w_usr+w_sub".to_string()),
            grant_type: Some("urn:ietf:params:oauth:grant-type:device_code".to_string()),
            ..Default::default()
        };
        let payload = serde_urlencoded::to_string(&req)?;
        let res = self
            .http_client
            .post(format!("{}/token", self.settings.oauth.base_url))
            .basic_auth(
                self.settings.oauth.client_id.clone(),
                Some(self.settings.oauth.client_secret.clone()),
            )
            .body(payload)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .send()
            .await?;
        if !res.status().is_success() {
            if res.status().is_client_error() {
                return Err(ClientError::AuthError(format!(
                    "Failed to check auth status: {}",
                    res.status().canonical_reason().unwrap_or("")
                )));
            } else {
                return Err(ClientError::AuthError(
                    "Failed to check auth status".to_string(),
                ));
            }
        }
        let refresh = res.json::<RefreshResponse>().await?;
        Ok(refresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Client {
        let settings = crate::config::Settings::default();
        Client::new(settings).unwrap()
    }

    #[tokio::test]
    async fn test_get_device_code() {
        let client = setup();
        println!("{:#?}", client);
        let response = client.get_device_code().await.unwrap();
        assert!(!response.device_code.is_empty());
        assert_eq!(response.device_code.len(), 36);
        assert!(!response.user_code.is_empty());
        assert_eq!(response.user_code.len(), 5);
        assert!(!response.verification_uri.is_empty());
        assert!(!response.verification_uri_complete.is_empty());
        assert!(response.expires_in == 300);
        assert!(response.interval != 0);
    }
}
