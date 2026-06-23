use crate::{MoatError, MoatResult};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct GoogleOAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct GoogleOAuthClient {
    client_id: String,
    client_secret: String,
    token_url: String,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

impl GoogleOAuthClient {
    pub fn new(
        client_id: String,
        client_secret: String,
        token_url: String,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            token_url,
        }
    }

    pub async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> MoatResult<GoogleOAuthCredentials> {
        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];
        
        let response = reqwest::Client::new()
            .post(&self.token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| {
                MoatError::Orchestrator(format!("Google OAuth token request failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let provider_response = response.text().await.unwrap_or_default();
            return Err(MoatError::Orchestrator(format!(
                "Google OAuth token exchange failed with status {status}: {provider_response}"
            )));
        }

        let data = response.json::<GoogleTokenResponse>().await.map_err(|e| {
            MoatError::Orchestrator(format!("Google OAuth token response parse failed: {e}"))
        })?;
        let expires_in = data.expires_in.unwrap_or(3600);

        Ok(GoogleOAuthCredentials {
            access_token: data.access_token,
            refresh_token: data.refresh_token,
            expires_at: chrono::Utc::now().timestamp() + expires_in,
        })
    }
}
