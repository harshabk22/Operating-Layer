#![allow(dead_code)]
use crate::agent::llm_types::{
    BoxFuture, LlmMessage, LlmProvider, LlmResponse, LlmTool, LlmToolCall,
};
use crate::config::{
    parse_user_document, user_md_path, ProviderHealth, DEEPSEEK_KEY_ACCOUNT, GOOGLE_KEY_ACCOUNT,
};
use crate::{MoatError, MoatResult};
use dashmap::DashMap;
use reqwest::Client;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(test)]
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ExternalApiConfig {
    pub google_key_env: String,
    pub deepseek_key_env: String,
    pub user_md_path: PathBuf,
    pub default_provider: Arc<Mutex<String>>,
}

impl Default for ExternalApiConfig {
    fn default() -> Self {
        Self {
            google_key_env: "GEMINI_API_KEY".into(),
            deepseek_key_env: "DEEPSEEK_API_KEY".into(),
            user_md_path: user_md_path(),
            default_provider: Arc::new(Mutex::new("deepseek".to_string())),
        }
    }
}

pub trait ProviderFactory: Send + Sync {
    fn provider_for(&self, api_provider: &str) -> MoatResult<Arc<dyn LlmProvider>>;
    fn routed_provider(
        &self,
        subsystem: &str,
        requested_model: &str,
    ) -> MoatResult<(Arc<dyn LlmProvider>, String)>;
    fn invalidate_cache(&self);
    fn has_stored_key(&self, api_provider: &str) -> MoatResult<bool>;
    fn set_provider_key(&self, api_provider: &str, api_key: &str) -> MoatResult<()>;
    fn remove_provider_key(&self, api_provider: &str) -> MoatResult<()>;
    fn set_default_provider(&self, provider: &str);
    fn get_default_provider(&self) -> String;
    fn provider_health<'a>(
        &'a self,
        api_provider: &'a str,
    ) -> BoxFuture<'a, MoatResult<ProviderHealth>>;
    fn provider_variant_health<'a>(
        &'a self,
        api_provider: &'a str,
        variant: Option<&'a str>,
    ) -> BoxFuture<'a, MoatResult<ProviderHealth>> {
        let _ = variant;
        self.provider_health(api_provider)
    }
}

pub trait SecretStore: Send + Sync {
    fn get_secret(&self, account: &str) -> MoatResult<Option<String>>;
    fn set_secret(&self, account: &str, secret: &str) -> MoatResult<()>;
    fn delete_secret(&self, account: &str) -> MoatResult<()>;
}

fn obfuscate(data: &str) -> String {
    let key = b"moatageex_secret_key_salt";
    let xored: Vec<u8> = data
        .as_bytes()
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect();
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, xored)
}

fn deobfuscate(data: &str) -> Option<String> {
    let key = b"moatageex_secret_key_salt";
    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data).ok()?;
    let dexored: Vec<u8> = decoded
        .iter()
        .enumerate()
        .map(|(i, &b)| b ^ key[i % key.len()])
        .collect();
    String::from_utf8(dexored).ok()
}

#[derive(Default)]
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn fallback_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".ageex")
            .join("secrets.bin")
    }

    fn read_fallback(&self, account: &str) -> MoatResult<Option<String>> {
        let path = self.fallback_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| MoatError::Settings(format!("Failed to read fallback secrets: {e}")))?;
        let map: std::collections::HashMap<String, String> =
            serde_json::from_str(&content).unwrap_or_default();
        if let Some(obfuscated) = map.get(account) {
            if let Some(decrypted) = deobfuscate(obfuscated) {
                return Ok(Some(decrypted));
            }
        }
        Ok(None)
    }

    fn write_fallback(&self, account: &str, secret: &str) -> MoatResult<()> {
        let path = self.fallback_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut map: std::collections::HashMap<String, String> = if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };
        map.insert(account.to_string(), obfuscate(secret));
        let content = serde_json::to_string(&map).map_err(|e| {
            MoatError::Settings(format!("Failed to serialize fallback secrets: {e}"))
        })?;
        std::fs::write(&path, content)
            .map_err(|e| MoatError::Settings(format!("Failed to write fallback secrets: {e}")))?;
        Ok(())
    }

    fn delete_fallback(&self, account: &str) -> MoatResult<()> {
        let path = self.fallback_path();
        if !path.exists() {
            return Ok(());
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut map: std::collections::HashMap<String, String> =
            serde_json::from_str(&content).unwrap_or_default();
        map.remove(account);
        let content = serde_json::to_string(&map).map_err(|e| {
            MoatError::Settings(format!("Failed to serialize fallback secrets: {e}"))
        })?;
        std::fs::write(&path, content)
            .map_err(|e| MoatError::Settings(format!("Failed to write fallback secrets: {e}")))?;
        Ok(())
    }
}

impl SecretStore for KeyringSecretStore {
    fn get_secret(&self, account: &str) -> MoatResult<Option<String>> {
        self.read_fallback(account)
    }

    fn set_secret(&self, account: &str, secret: &str) -> MoatResult<()> {
        self.write_fallback(account, secret)
    }

    fn delete_secret(&self, account: &str) -> MoatResult<()> {
        self.delete_fallback(account)
    }
}

pub struct ExternalProviderFactory {
    client: Client,
    config: ExternalApiConfig,
    cache: DashMap<String, Arc<dyn LlmProvider>>,
    secrets: Arc<dyn SecretStore>,
}

impl ExternalProviderFactory {
    pub fn new(config: ExternalApiConfig) -> Self {
        Self::with_secret_store(config, Arc::new(KeyringSecretStore))
    }

    pub fn with_secret_store(config: ExternalApiConfig, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            client: Client::new(),
            config,
            cache: DashMap::new(),
            secrets,
        }
    }

    fn resolve_for_subsystem(&self, subsystem: &str) -> MoatResult<Arc<dyn LlmProvider>> {
        let provider_name = self.config.default_provider.lock().unwrap().clone();
        eprintln!(
            "[DEL] resolve_for_subsystem({}) → provider={}",
            subsystem, provider_name
        );
        self.provider_for(&provider_name).map_err(|err| {
            MoatError::Llm(format!(
                "no provider available for subsystem '{subsystem}': {err}"
            ))
        })
    }

    fn google_env_key(&self) -> &str {
        self.config.google_key_env.as_str()
    }

    fn deepseek_env_key(&self) -> &str {
        self.config.deepseek_key_env.as_str()
    }

    fn read_workspace_env_key(&self, key_name: &str) -> Option<String> {
        if let Some(parent) = self.config.user_md_path.parent() {
            let env_path = parent.join(".env");
            if let Ok(content) = std::fs::read_to_string(env_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') || trimmed.is_empty() {
                        continue;
                    }
                    if let Some((k, v)) = trimmed.split_once('=') {
                        if k.trim() == key_name {
                            let val = v.trim();
                            let val = if (val.starts_with('"') && val.ends_with('"'))
                                || (val.starts_with('\'') && val.ends_with('\''))
                            {
                                &val[1..val.len() - 1]
                            } else {
                                val
                            };
                            let val = val.trim().to_string();
                            if !val.is_empty() {
                                return Some(val);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn read_google_frontmatter_key(&self) -> MoatResult<Option<String>> {
        match std::fs::read_to_string(&self.config.user_md_path) {
            Ok(markdown) => {
                let doc = parse_user_document(&markdown)?;
                Ok(doc
                    .frontmatter
                    .google_api_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(MoatError::Settings(format!(
                "failed to read USER.md for Google key resolution: {err}"
            ))),
        }
    }

    fn read_deepseek_frontmatter_key(&self) -> MoatResult<Option<String>> {
        match std::fs::read_to_string(&self.config.user_md_path) {
            Ok(markdown) => {
                let doc = parse_user_document(&markdown)?;
                Ok(doc
                    .frontmatter
                    .deepseek_api_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(MoatError::Settings(format!(
                "failed to read USER.md for DeepSeek key resolution: {err}"
            ))),
        }
    }

    fn read_google_credentials(&self) -> MoatResult<Vec<GoogleCredential>> {
        let mut credentials = Vec::new();

        let env_key = self.google_env_key();

        if let Some(api_key) = self.read_workspace_env_key(env_key) {
            credentials.push(GoogleCredential {
                profile_id: "workspace-env".to_string(),
                label: "Workspace .env Google".to_string(),
                model_hint: String::new(),
                api_key,
            });
        }

        if let Some(api_key) = self.read_google_frontmatter_key()? {
            credentials.push(GoogleCredential {
                profile_id: "user-md".to_string(),
                label: "USER.md Google".to_string(),
                model_hint: String::new(),
                api_key,
            });
        }

        let env_secret = env::var(env_key)
            .map(|value| value.trim().to_string())
            .ok()
            .filter(|value| !value.is_empty());
        if let Some(api_key) = env_secret {
            credentials.push(GoogleCredential {
                profile_id: "env".to_string(),
                label: "Environment Google".to_string(),
                model_hint: String::new(),
                api_key,
            });
        }

        if let Some(secret) = self.secrets.get_secret(GOOGLE_KEY_ACCOUNT)? {
            let trimmed = secret.trim();
            if !trimmed.is_empty() {
                credentials.push(GoogleCredential {
                    profile_id: "secret-store".to_string(),
                    label: "Secret Store Google".to_string(),
                    model_hint: String::new(),
                    api_key: trimmed.to_string(),
                });
            }
        }

        if !credentials.is_empty() {
            Ok(credentials)
        } else {
            Err(missing_api_key_error("google", env_key))
        }
    }

    fn read_deepseek_credentials(&self) -> MoatResult<Vec<DeepSeekCredential>> {
        let mut credentials = Vec::new();

        let env_key = self.deepseek_env_key();

        if let Some(api_key) = self.read_workspace_env_key(env_key) {
            credentials.push(DeepSeekCredential {
                profile_id: "workspace-env".to_string(),
                label: "Workspace .env DeepSeek".to_string(),
                model_hint: String::new(),
                api_key,
            });
        }

        if let Some(api_key) = self.read_deepseek_frontmatter_key()? {
            credentials.push(DeepSeekCredential {
                profile_id: "user-md".to_string(),
                label: "USER.md DeepSeek".to_string(),
                model_hint: String::new(),
                api_key,
            });
        }

        let env_secret = env::var(env_key)
            .map(|value| value.trim().to_string())
            .ok()
            .filter(|value| !value.is_empty());
        if let Some(api_key) = env_secret {
            credentials.push(DeepSeekCredential {
                profile_id: "env".to_string(),
                label: "Environment DeepSeek".to_string(),
                model_hint: String::new(),
                api_key,
            });
        }

        if let Some(secret) = self.secrets.get_secret(DEEPSEEK_KEY_ACCOUNT)? {
            let trimmed = secret.trim();
            if !trimmed.is_empty() {
                credentials.push(DeepSeekCredential {
                    profile_id: "secret-store".to_string(),
                    label: "Secret Store DeepSeek".to_string(),
                    model_hint: String::new(),
                    api_key: trimmed.to_string(),
                });
            }
        }

        if !credentials.is_empty() {
            Ok(credentials)
        } else {
            Err(missing_api_key_error("deepseek", env_key))
        }
    }

    async fn provider_health_for_variant(
        &self,
        api_provider: &str,
        variant: Option<&str>,
    ) -> MoatResult<ProviderHealth> {
        if variant.is_some() {
            return Ok(ProviderHealth::Missing);
        }
        match api_provider.trim().to_ascii_lowercase().as_str() {
            "google" => match self.provider_for("google") {
                Ok(provider) => match provider.check_health().await {
                    Ok(()) => Ok(ProviderHealth::Valid),
                    Err(_) => Ok(ProviderHealth::Invalid),
                },
                Err(err) => {
                    if missing_key_provider_from_error(&err).is_some() {
                        Ok(ProviderHealth::Missing)
                    } else {
                        Err(err)
                    }
                }
            },
            "deepseek" => match self.provider_for("deepseek") {
                Ok(provider) => match provider.check_health().await {
                    Ok(()) => Ok(ProviderHealth::Valid),
                    Err(_) => Ok(ProviderHealth::Invalid),
                },
                Err(err) => {
                    if missing_key_provider_from_error(&err).is_some() {
                        Ok(ProviderHealth::Missing)
                    } else {
                        Err(err)
                    }
                }
            },
            other => Err(MoatError::Llm(format!(
                "unsupported external provider '{other}'; MoatAgeex supports google and deepseek"
            ))),
        }
    }
}

impl ProviderFactory for ExternalProviderFactory {
    fn provider_for(&self, api_provider: &str) -> MoatResult<Arc<dyn LlmProvider>> {
        let key = api_provider.trim().to_ascii_lowercase();
        if let Some(existing) = self.cache.get(&key) {
            return Ok(Arc::clone(existing.value()));
        }

        let provider: Arc<dyn LlmProvider> = match key.as_str() {
            "google" => Arc::new(GoogleMultiClient {
                client: self.client.clone(),
                credentials: self.read_google_credentials()?,
            }),
            "deepseek" => Arc::new(DeepSeekMultiClient {
                client: self.client.clone(),
                credentials: self.read_deepseek_credentials()?,
            }),
            other => {
                return Err(MoatError::Llm(format!(
                "unsupported external provider '{other}'; MoatAgeex supports google and deepseek"
            )))
            }
        };

        self.cache.insert(key, Arc::clone(&provider));
        Ok(provider)
    }

    fn routed_provider(
        &self,
        subsystem: &str,
        requested_model: &str,
    ) -> MoatResult<(Arc<dyn LlmProvider>, String)> {
        let provider = self.resolve_for_subsystem(subsystem)?;
        Ok((provider, requested_model.to_string()))
    }

    fn invalidate_cache(&self) {
        self.cache.clear();
    }

    fn has_stored_key(&self, api_provider: &str) -> MoatResult<bool> {
        let normalized = api_provider.trim().to_ascii_lowercase();
        if normalized.starts_with("google") {
            let has_workspace_env = self.read_workspace_env_key(self.google_env_key()).is_some();
            let has_frontmatter = self.read_google_frontmatter_key()?.is_some();
            let has_env = env::var(self.google_env_key()).is_ok();
            let has_secrets = self
                .secrets
                .get_secret(GOOGLE_KEY_ACCOUNT)?
                .map(|val| !val.trim().is_empty())
                .unwrap_or(false);
            Ok(has_workspace_env || has_frontmatter || has_env || has_secrets)
        } else if normalized.starts_with("deepseek") {
            let has_workspace_env = self
                .read_workspace_env_key(self.deepseek_env_key())
                .is_some();
            let has_frontmatter = self.read_deepseek_frontmatter_key()?.is_some();
            let has_env = env::var(self.deepseek_env_key()).is_ok();
            let has_secrets = self
                .secrets
                .get_secret(DEEPSEEK_KEY_ACCOUNT)?
                .map(|val| !val.trim().is_empty())
                .unwrap_or(false);
            Ok(has_workspace_env || has_frontmatter || has_env || has_secrets)
        } else {
            Ok(false)
        }
    }

    fn set_provider_key(&self, api_provider: &str, api_key: &str) -> MoatResult<()> {
        let normalized = api_provider.trim().to_ascii_lowercase();
        let account = if normalized.starts_with("google") {
            GOOGLE_KEY_ACCOUNT.to_string()
        } else if normalized.starts_with("deepseek") {
            DEEPSEEK_KEY_ACCOUNT.to_string()
        } else {
            return Err(MoatError::Settings(format!(
                "unsupported provider '{api_provider}' for secret store"
            )));
        };
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        self.secrets.set_secret(&account, trimmed)
    }

    fn remove_provider_key(&self, api_provider: &str) -> MoatResult<()> {
        let normalized = api_provider.trim().to_ascii_lowercase();
        let account = if normalized.starts_with("google") {
            GOOGLE_KEY_ACCOUNT.to_string()
        } else if normalized.starts_with("deepseek") {
            DEEPSEEK_KEY_ACCOUNT.to_string()
        } else {
            return Err(MoatError::Settings(format!(
                "unsupported provider '{api_provider}' for secret store"
            )));
        };
        self.secrets.delete_secret(&account)
    }

    fn set_default_provider(&self, provider: &str) {
        eprintln!("[DEL] set_default_provider → {}", provider);
        *self.config.default_provider.lock().unwrap() = provider.to_string();
        self.invalidate_cache();
    }

    fn get_default_provider(&self) -> String {
        self.config.default_provider.lock().unwrap().clone()
    }

    fn provider_health<'a>(
        &'a self,
        api_provider: &'a str,
    ) -> BoxFuture<'a, MoatResult<ProviderHealth>> {
        Box::pin(async move { self.provider_health_for_variant(api_provider, None).await })
    }

    fn provider_variant_health<'a>(
        &'a self,
        api_provider: &'a str,
        variant: Option<&'a str>,
    ) -> BoxFuture<'a, MoatResult<ProviderHealth>> {
        Box::pin(async move {
            self.provider_health_for_variant(api_provider, variant)
                .await
        })
    }
}

const MISSING_API_KEY_PREFIX: &str = "missing_api_key::";

fn missing_api_key_error(api_provider: &str, env_key: &str) -> MoatError {
    MoatError::Llm(format!("{MISSING_API_KEY_PREFIX}{api_provider}::{env_key}"))
}

pub fn missing_key_provider_from_error(error: &MoatError) -> Option<String> {
    missing_key_provider_from_text(&error.to_string())
}

pub fn missing_key_provider_from_text(text: &str) -> Option<String> {
    let start = text.find(MISSING_API_KEY_PREFIX)? + MISSING_API_KEY_PREFIX.len();
    let rest = &text[start..];
    let provider = rest.split("::").next()?.trim();
    if provider.is_empty() {
        None
    } else {
        Some(provider.to_string())
    }
}

pub fn provider_display_name(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        "google" => "Google",
        "deepseek" => "DeepSeek",
        _ => "External",
    }
}

#[derive(Clone)]
struct GoogleCredential {
    profile_id: String,
    label: String,
    model_hint: String,
    api_key: String,
}

fn google_model_match_score(requested_model: &str, model_hint: &str) -> u8 {
    let requested = requested_model.trim().to_ascii_lowercase();
    let hint = model_hint.trim().to_ascii_lowercase();
    if requested.is_empty() || hint.is_empty() {
        return 0;
    }
    if requested == hint {
        return 3;
    }
    if requested.contains(&hint) || hint.contains(&requested) {
        return 2;
    }
    let requested_slug = requested.replace(['/', ':'], "-");
    let hint_slug = hint.replace(['/', ':'], "-");
    if requested_slug.contains(&hint_slug) || hint_slug.contains(&requested_slug) {
        return 1;
    }
    0
}

#[derive(Clone)]
struct DeepSeekCredential {
    profile_id: String,
    label: String,
    model_hint: String,
    api_key: String,
}

fn deepseek_model_match_score(requested_model: &str, model_hint: &str) -> u8 {
    let requested = requested_model.trim().to_ascii_lowercase();
    let hint = model_hint.trim().to_ascii_lowercase();
    if requested.is_empty() || hint.is_empty() {
        return 0;
    }
    if requested == hint {
        return 3;
    }
    if requested.contains(&hint) || hint.contains(&requested) {
        return 2;
    }
    0
}

async fn read_json_response(response: reqwest::Response, provider: &str) -> MoatResult<Value> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| MoatError::Llm(format!("{provider} response read failed: {err}")))?;

    if status.as_u16() == 429 {
        return Err(MoatError::Llm(format!(
            "{provider} rate limit hit (429). Slow down or rotate the provider."
        )));
    }

    if !status.is_success() {
        return Err(MoatError::Llm(format!(
            "{provider} API error {}: {}",
            status.as_u16(),
            body.chars().take(256).collect::<String>()
        )));
    }

    serde_json::from_str(&body)
        .map_err(|err| MoatError::Llm(format!("{provider} JSON parse failed: {err}")))
}

async fn read_health_response(response: reqwest::Response, provider: &str) -> MoatResult<()> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| MoatError::Llm(format!("{provider} health check failed: {err}")))?;

    if !status.is_success() {
        return Err(MoatError::Llm(format!(
            "{provider} health check returned {}: {}",
            status.as_u16(),
            body.chars().take(128).collect::<String>()
        )));
    }

    Ok(())
}

fn extract_chat_completion_content(payload: &Value, provider: &str) -> MoatResult<(String, u32)> {
    let text = payload
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| MoatError::Llm(format!("{provider} response missing assistant content")))?
        .to_string();

    let tokens = payload
        .get("usage")
        .and_then(|usage| usage.get("total_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    Ok((text, tokens))
}

struct GoogleExternalClient {
    client: Client,
    api_key: String,
}

const GOOGLE_INFERENCE_HEALTHCHECK_MODEL: &str = "gemini-2.5-flash";
const DEEPSEEK_INFERENCE_HEALTHCHECK_MODEL: &str = "deepseek-chat";

struct GoogleMultiClient {
    client: Client,
    credentials: Vec<GoogleCredential>,
}

impl LlmProvider for GoogleMultiClient {
    fn check_health<'a>(&'a self) -> BoxFuture<'a, MoatResult<()>> {
        Box::pin(async move {
            for credential in &self.credentials {
                let client = GoogleExternalClient {
                    client: self.client.clone(),
                    api_key: credential.api_key.clone(),
                };
                if client.check_health().await.is_ok() {
                    return Ok(());
                }
            }
            Err(MoatError::Llm(
                "all configured Google credentials failed health checks".into(),
            ))
        })
    }
    fn chat_complete<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
        max_tokens: usize,
        temperature: f32,
        response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        Box::pin(async move {
            let mut errors = Vec::new();
            let mut ordered_credentials = self.credentials.clone();
            ordered_credentials.sort_by(|left, right| {
                let left_score = google_model_match_score(model, &left.model_hint);
                let right_score = google_model_match_score(model, &right.model_hint);
                right_score
                    .cmp(&left_score)
                    .then_with(|| left.label.cmp(&right.label))
            });

            for credential in &ordered_credentials {
                let client = GoogleExternalClient {
                    client: self.client.clone(),
                    api_key: credential.api_key.clone(),
                };
                let target_model = if credential.model_hint.trim().is_empty() {
                    if model.to_ascii_lowercase().contains("gemini") {
                        model
                    } else {
                        "gemini-2.5-flash"
                    }
                } else {
                    credential.model_hint.trim()
                };
                match client
                    .chat_complete(
                        target_model,
                        messages.clone(),
                        tools.clone(),
                        max_tokens,
                        temperature,
                        response_format.clone(),
                    )
                    .await
                {
                    Ok(result) => return Ok(result),
                    Err(err) => errors.push(format!(
                        "{} [{}] {}{}: {}",
                        credential.label,
                        credential.profile_id,
                        provider_display_name("google"),
                        if credential.model_hint.trim().is_empty() {
                            String::new()
                        } else {
                            format!(" ({})", credential.model_hint.trim())
                        },
                        err
                    )),
                }
            }

            Err(MoatError::Llm(format!(
                "No Google credential succeeded for chat_complete. Tried: {}",
                errors.join(" | ")
            )))
        })
    }
}

impl LlmProvider for GoogleExternalClient {
    fn chat_complete<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
        max_tokens: usize,
        temperature: f32,
        response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        let api_key = self.api_key.clone();
        let client = self.client.clone();
        let model = model.to_string();

        Box::pin(async move {
            let mut payload = json!({
                "model": model,
                "messages": messages,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "stream": false,
            });

            if let Some(t) = tools {
                if !t.is_empty() {
                    payload["tools"] = serde_json::to_value(t).unwrap();
                }
            }

            if let Some(rf) = response_format {
                payload["response_format"] = json!({
                    "type": "json_schema",
                    "json_schema": {
                        "name": "agent_response",
                        "strict": true,
                        "schema": rf
                    }
                });
            }

            let response = client
                .post("https://generativelanguage.googleapis.com/v1beta/chat/completions")
                .bearer_auth(&api_key)
                .timeout(std::time::Duration::from_secs(60))
                .json(&payload)
                .send()
                .await
                .map_err(|err| {
                    MoatError::Llm(format!("google request failed or timed out: {err}"))
                })?;

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(MoatError::Llm(format!(
                    "google API error {}: {}",
                    status.as_u16(),
                    body.chars().take(256).collect::<String>()
                )));
            }

            let json: serde_json::Value = response
                .json()
                .await
                .map_err(|err| MoatError::Llm(format!("failed to parse google response: {err}")))?;

            let choice = json
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.first())
                .ok_or_else(|| MoatError::Llm("google response missing choices".into()))?;

            let message = choice
                .get("message")
                .ok_or_else(|| MoatError::Llm("google response missing message".into()))?;

            let content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map(|s| s.to_string());

            let tool_calls: Option<Vec<LlmToolCall>> =
                if let Some(tc_val) = message.get("tool_calls") {
                    serde_json::from_value(tc_val.clone()).ok()
                } else {
                    None
                };

            let structured = content.as_deref().and_then(|c| {
                serde_json::from_str::<crate::agent::llm_types::AgentStructuredResponse>(c).ok()
            });

            let tokens = json
                .get("usage")
                .and_then(|usage| usage.get("total_tokens"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;

            Ok(LlmResponse {
                content,
                tool_calls,
                tokens,
                structured,
            })
        })
    }

    fn check_health<'a>(&'a self) -> BoxFuture<'a, MoatResult<()>> {
        Box::pin(async move {
            let response = self
                .client
                .post("https://generativelanguage.googleapis.com/v1beta/chat/completions")
                .bearer_auth(&self.api_key)
                .json(&json!({
                    "model": GOOGLE_INFERENCE_HEALTHCHECK_MODEL,
                    "messages": [
                        {"role": "user", "content": "ping"}
                    ],
                    "max_tokens": 1,
                    "temperature": 0.0,
                }))
                .send()
                .await
                .map_err(|err| {
                    MoatError::Llm(format!("google inference health request failed: {err}"))
                })?;
            read_health_response(response, "google").await
        })
    }
}

// ─── DeepSeek Provider ────────────────────────────────────────────────

struct DeepSeekExternalClient {
    client: Client,
    api_key: String,
}

struct DeepSeekMultiClient {
    client: Client,
    credentials: Vec<DeepSeekCredential>,
}

impl LlmProvider for DeepSeekMultiClient {
    fn chat_complete<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
        max_tokens: usize,
        temperature: f32,
        response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        Box::pin(async move {
            let mut errors = Vec::new();
            let mut ordered_credentials = self.credentials.clone();
            ordered_credentials.sort_by(|left, right| {
                let left_score = deepseek_model_match_score(model, &left.model_hint);
                let right_score = deepseek_model_match_score(model, &right.model_hint);
                right_score
                    .cmp(&left_score)
                    .then_with(|| left.label.cmp(&right.label))
            });

            for credential in ordered_credentials {
                let client = DeepSeekExternalClient {
                    client: self.client.clone(),
                    api_key: credential.api_key.clone(),
                };
                let target_model = if credential.model_hint.trim().is_empty() {
                    if model.to_ascii_lowercase().contains("deepseek") {
                        model
                    } else {
                        "deepseek-chat"
                    }
                } else {
                    credential.model_hint.trim()
                };
                match client
                    .chat_complete(
                        target_model,
                        messages.clone(),
                        tools.clone(),
                        max_tokens,
                        temperature,
                        response_format.clone(),
                    )
                    .await
                {
                    Ok(result) => return Ok(result),
                    Err(err) => {
                        errors.push(format!(
                            "{}{}: {err}",
                            credential.label,
                            if credential.model_hint.trim().is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", credential.model_hint.trim())
                            },
                        ));
                    }
                }
            }

            Err(MoatError::Llm(format!(
                "No DeepSeek credential succeeded for chat_complete. Tried: {}",
                errors.join(" | ")
            )))
        })
    }
}

impl LlmProvider for DeepSeekExternalClient {
    fn chat_complete<'a>(
        &'a self,
        model: &'a str,
        messages: Vec<LlmMessage>,
        tools: Option<Vec<LlmTool>>,
        max_tokens: usize,
        temperature: f32,
        response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, MoatResult<LlmResponse>> {
        let api_key = self.api_key.clone();
        let client = self.client.clone();
        let model = model.to_string();

        Box::pin(async move {
            let thinking_enabled = std::env::var("AGEEX_THINKING")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false);
            let mut payload = json!({
                "model": model,
                "messages": messages,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "stream": false,
            });
            if thinking_enabled {
                payload["thinking"] = json!({"type": "enabled"});
            }

            if let Some(mut t) = tools {
                if !t.is_empty() {
                    for tool in &mut t {
                        tool.function.name = tool.function.name.replace(':', "__");
                    }
                    payload["tools"] = serde_json::to_value(&t).unwrap();
                }
            }

            if response_format.is_some() {
                payload["response_format"] = json!({
                    "type": "json_object"
                });
            }

            let response = client
                .post("https://api.deepseek.com/chat/completions")
                .bearer_auth(&api_key)
                .timeout(std::time::Duration::from_secs(60))
                .json(&payload)
                .send()
                .await
                .map_err(|err| {
                    MoatError::Llm(format!("deepseek request failed or timed out: {err}"))
                })?;

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(MoatError::Llm(format!(
                    "deepseek API error {}: {}",
                    status.as_u16(),
                    body.chars().take(256).collect::<String>()
                )));
            }

            let json: serde_json::Value = response.json().await.map_err(|err| {
                MoatError::Llm(format!("failed to parse deepseek response: {err}"))
            })?;

            let choice = json
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|c| c.first())
                .ok_or_else(|| MoatError::Llm("deepseek response missing choices".into()))?;

            let message = choice
                .get("message")
                .ok_or_else(|| MoatError::Llm("deepseek response missing message".into()))?;

            let mut content = message
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map(|s| s.to_string());
            if let Some(ref c) = content {
                if c.contains("__") {
                    content = Some(c.replace("__", ":"));
                }
            }

            let mut tool_calls: Option<Vec<LlmToolCall>> =
                if let Some(tc_val) = message.get("tool_calls") {
                    serde_json::from_value(tc_val.clone()).ok()
                } else {
                    None
                };
            if let Some(ref mut calls) = tool_calls {
                for call in calls {
                    call.function.name = call.function.name.replace("__", ":");
                }
            }

            let structured = content.as_deref().and_then(|c| {
                serde_json::from_str::<crate::agent::llm_types::AgentStructuredResponse>(c).ok()
            });

            let tokens = json
                .get("usage")
                .and_then(|usage| usage.get("total_tokens"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;

            Ok(LlmResponse {
                content,
                tool_calls,
                tokens,
                structured,
            })
        })
    }

    fn check_health<'a>(&'a self) -> BoxFuture<'a, MoatResult<()>> {
        Box::pin(async move {
            let response = self
                .client
                .post("https://api.deepseek.com/chat/completions")
                .bearer_auth(&self.api_key)
                .json(&json!({
                    "model": DEEPSEEK_INFERENCE_HEALTHCHECK_MODEL,
                    "messages": [
                        {"role": "user", "content": "ping"}
                    ],
                    "max_tokens": 1,
                    "temperature": 0.0,
                }))
                .send()
                .await
                .map_err(|err| {
                    MoatError::Llm(format!("deepseek inference health request failed: {err}"))
                })?;
            read_health_response(response, "deepseek").await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[derive(Default)]
    struct InMemorySecretStore {
        secrets: Mutex<HashMap<String, String>>,
    }

    impl SecretStore for InMemorySecretStore {
        fn get_secret(&self, account: &str) -> MoatResult<Option<String>> {
            Ok(self
                .secrets
                .lock()
                .expect("lock secrets")
                .get(account)
                .cloned())
        }

        fn set_secret(&self, account: &str, secret: &str) -> MoatResult<()> {
            self.secrets
                .lock()
                .expect("lock secrets")
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn delete_secret(&self, account: &str) -> MoatResult<()> {
            self.secrets.lock().expect("lock secrets").remove(account);
            Ok(())
        }
    }

    #[test]
    fn provider_factory_prefers_user_md_then_env_then_secret_store_for_google() {
        let store = Arc::new(InMemorySecretStore::default());
        store
            .set_secret(GOOGLE_KEY_ACCOUNT, "stored-google")
            .expect("store secret");
        let user_md_path = std::env::temp_dir().join(format!(
            "moat_external_api_test_{}_USER.md",
            uuid::Uuid::new_v4()
        ));
        fs::write(
            &user_md_path,
            "---\nname: \"User\"\ncore_preferences: \"\"\nchat_model: null\nplanner_model: null\nworker_model: null\ngoogle_api_key: frontmatter-google\n---\n",
        )
        .expect("write USER.md");
        unsafe {
            std::env::set_var("TEST_GEMINI_API_KEY_PREFER", "env-google");
        }

        let factory = ExternalProviderFactory::with_secret_store(
            ExternalApiConfig {
                google_key_env: "TEST_GEMINI_API_KEY_PREFER".to_string(),
                user_md_path: user_md_path.clone(),
                ..ExternalApiConfig::default()
            },
            store,
        );
        let credentials = factory.read_google_credentials().expect("read credentials");
        assert!(!credentials.is_empty());
        assert_eq!(credentials[0].profile_id, "user-md");
        assert_eq!(credentials[0].api_key, "frontmatter-google");

        let _ = fs::remove_file(user_md_path);
        unsafe {
            std::env::remove_var("TEST_GEMINI_API_KEY_PREFER");
        }
    }

    #[test]
    fn provider_factory_can_set_and_remove_google_keys() {
        let store = Arc::new(InMemorySecretStore::default());
        let factory = ExternalProviderFactory::with_secret_store(
            ExternalApiConfig {
                google_key_env: "TEST_GEMINI_API_KEY_STORED".to_string(),
                ..ExternalApiConfig::default()
            },
            store,
        );

        unsafe {
            std::env::remove_var("TEST_GEMINI_API_KEY_STORED");
        }
        assert!(!factory.has_stored_key("google").expect("missing key"));

        unsafe {
            std::env::set_var("TEST_GEMINI_API_KEY_STORED", "env-google");
        }
        assert!(factory.has_stored_key("google").expect("saved key"));

        unsafe {
            std::env::remove_var("TEST_GEMINI_API_KEY_STORED");
        }
        assert!(!factory.has_stored_key("google").expect("removed key"));
    }
}
