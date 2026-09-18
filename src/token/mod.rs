//! Local Android token parsing and in-memory storage.
//!
//! The helper intentionally does not discover credentials from browser
//! profiles or persist them.  The extension sends the contents of a token
//! JSON file after the user explicitly chooses that file in its preferences.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::error::HelperError;

const MAX_TOKEN_JSON_BYTES: usize = 1 * 1024 * 1024;
const MAX_SECRET_BYTES: usize = 4 * 1024;
const REFRESH_SKEW_SECONDS: u64 = 60;

/// A validated Android access token kept in memory while the helper is running.
///
/// The secret fields are private and the Debug implementation redacts them so
/// accidental logs cannot expose credentials.
#[derive(Clone)]
pub struct AndroidToken {
    access_key: String,
    refresh_token: Option<String>,
    mid: Option<String>,
    expires_at: Option<u64>,
    buvid: Option<String>,
    fast_login_token: Option<String>,
    app_key: Option<String>,
}

#[derive(serde::Serialize)]
struct StoredToken<'a> {
    access_key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    buvid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fast_login_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_key: Option<&'a str>,
}

impl fmt::Debug for AndroidToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AndroidToken")
            .field("access_key", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("mid", &self.mid)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl AndroidToken {
    pub(crate) fn access_key(&self) -> &str {
        &self.access_key
    }

    pub fn has_refresh_token(&self) -> bool {
        self.refresh_token.is_some()
    }

    pub fn mid(&self) -> Option<&str> {
        self.mid.as_deref()
    }

    pub fn expires_at(&self) -> Option<u64> {
        self.expires_at
    }

    pub(crate) fn buvid(&self) -> Option<&str> {
        self.buvid.as_deref()
    }

    pub(crate) fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    pub(crate) fn app_key(&self) -> Option<&str> {
        self.app_key.as_deref()
    }

    pub(crate) fn set_buvid(&mut self, buvid: String) -> Result<(), HelperError> {
        validate_secret("buvid", &buvid)?;
        self.buvid = Some(buvid);
        Ok(())
    }

    pub fn is_expired_at(&self, unix_seconds: u64) -> bool {
        self.expires_at
            .map(|expires_at| expires_at <= unix_seconds)
            .unwrap_or(false)
    }

    pub fn needs_refresh_at(&self, unix_seconds: u64) -> bool {
        self.expires_at
            .map(|expires_at| expires_at <= unix_seconds.saturating_add(REFRESH_SKEW_SECONDS))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenStatus {
    pub configured: bool,
    pub has_refresh_token: bool,
    pub expires_at: Option<u64>,
}

pub struct TokenStore {
    token: Option<AndroidToken>,
    storage_path: Option<PathBuf>,
}

impl TokenStore {
    pub fn open_default() -> Self {
        let mut store = Self {
            token: None,
            storage_path: default_storage_path(),
        };
        let _ = store.load();
        store
    }

    pub fn load(&mut self) -> Result<(), HelperError> {
        let Some(path) = self.storage_path.as_deref() else {
            return Ok(());
        };
        if !path.is_file() {
            return Ok(());
        }
        let json = fs::read_to_string(path)?;
        self.token = Some(parse_token_json(&json)?);
        Ok(())
    }

    pub fn import_json(&mut self, json: &str) -> Result<TokenStatus, HelperError> {
        let token = parse_token_json(json)?;
        if let Some(path) = self.storage_path.as_deref() {
            persist_token(path, &token)?;
        }
        self.token = Some(token);
        Ok(self.status())
    }

    pub fn clear(&mut self) {
        // Drop the String values as soon as possible.  This does not claim to
        // provide secure memory erasure; it simply avoids retaining old data.
        self.token = None;
        if let Some(path) = self.storage_path.as_deref() {
            let _ = fs::remove_file(path);
        }
    }

    pub fn status(&self) -> TokenStatus {
        match self.token.as_ref() {
            Some(token) => TokenStatus {
                configured: true,
                has_refresh_token: token.has_refresh_token(),
                expires_at: token.expires_at(),
            },
            None => TokenStatus {
                configured: false,
                has_refresh_token: false,
                expires_at: None,
            },
        }
    }

    pub(crate) fn token(&self) -> Option<&AndroidToken> {
        self.token.as_ref()
    }

    pub fn set_buvid(&mut self, buvid: String) -> Result<TokenStatus, HelperError> {
        self.token
            .as_mut()
            .ok_or(HelperError::MissingToken)?
            .set_buvid(buvid)?;
        if let Some(path) = self.storage_path.as_deref() {
            let token = self.token.as_ref().ok_or(HelperError::MissingToken)?;
            persist_token(path, token)?;
        }
        Ok(self.status())
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self {
            token: None,
            storage_path: None,
        }
    }
}

fn default_storage_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(path))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
    }?;
    Some(base.join("biliwebandroidstream").join("token.json"))
}

fn persist_token(path: &Path, token: &AndroidToken) -> Result<(), HelperError> {
    let parent = path
        .parent()
        .ok_or_else(|| HelperError::InvalidToken("token storage path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let stored = StoredToken {
        access_key: &token.access_key,
        refresh_token: token.refresh_token.as_deref(),
        mid: token.mid.as_deref(),
        expires_at: token.expires_at,
        buvid: token.buvid(),
        fast_login_token: token.fast_login_token.as_deref(),
        app_key: token.app_key.as_deref(),
    };
    let bytes = serde_json::to_vec_pretty(&stored)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(temp, path)?;
    Ok(())
}

pub fn parse_token_json(json: &str) -> Result<AndroidToken, HelperError> {
    if json.len() > MAX_TOKEN_JSON_BYTES {
        return Err(HelperError::InvalidToken(format!(
            "JSON exceeds {MAX_TOKEN_JSON_BYTES} bytes"
        )));
    }

    let value: Value = serde_json::from_str(json)
        .map_err(|error| HelperError::InvalidToken(format!("malformed JSON: {error}")))?;
    let object = token_object(&value).ok_or_else(|| {
        HelperError::InvalidToken(
            "expected a JSON object containing access_key or access_token".into(),
        )
    })?;

    let access_key = string_field(
        object,
        &["access_key", "accessKey", "access_token", "accessToken"],
    )
    .ok_or_else(|| HelperError::InvalidToken("missing access_key/access_token".into()))?;
    validate_secret("access key", &access_key)?;

    let refresh_token = string_field(object, &["refresh_token", "refreshToken"])
        .map(|value| value.to_owned())
        .filter(|value| !value.is_empty());
    if let Some(refresh_token) = refresh_token.as_ref() {
        validate_secret("refresh token", refresh_token)?;
    }

    let mid = string_or_number_field(object, &["mid", "uid", "user_id", "userId"]);
    let expires_at = number_field(
        object,
        &[
            "expires_at",
            "expiresAt",
            "expire_at",
            "expireAt",
            "expires",
        ],
    )
    .or_else(|| {
        number_field(
            object,
            &["expire_in", "expires_in", "expireIn", "expiresIn"],
        )
        .map(|seconds| now_ts().saturating_add(seconds))
    });
    let buvid = string_field(object, &["buvid", "buvid3", "buvid4"]).map(str::to_owned);
    let fast_login_token = string_field(object, &["fast_login_token", "fastLoginToken"])
        .map(str::to_owned)
        .filter(|value| !value.is_empty());
    let app_key = string_field(object, &["app_key", "appKey"])
        .map(str::to_owned)
        .filter(|value| !value.is_empty());

    Ok(AndroidToken {
        access_key: access_key.to_owned(),
        refresh_token,
        mid,
        expires_at,
        buvid,
        fast_login_token,
        app_key,
    })
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn token_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    let object = value.as_object()?;
    if has_any_field(
        object,
        &["access_key", "accessKey", "access_token", "accessToken"],
    ) {
        return Some(object);
    }

    for wrapper in ["data", "token", "token_info", "result"] {
        if let Some(nested) = object.get(wrapper).and_then(Value::as_object) {
            if has_any_field(
                nested,
                &["access_key", "accessKey", "access_token", "accessToken"],
            ) {
                return Some(nested);
            }
        }
    }
    None
}

fn has_any_field(object: &serde_json::Map<String, Value>, names: &[&str]) -> bool {
    names.iter().any(|name| object.contains_key(*name))
}

fn string_field<'a>(object: &'a serde_json::Map<String, Value>, names: &[&str]) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| object.get(*name).and_then(Value::as_str))
}

fn string_or_number_field(
    object: &serde_json::Map<String, Value>,
    names: &[&str],
) -> Option<String> {
    names.iter().find_map(|name| {
        object.get(*name).and_then(|value| match value {
            Value::String(value) if !value.is_empty() => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
    })
}

fn number_field(object: &serde_json::Map<String, Value>, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        object.get(*name).and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(value) => value.parse::<u64>().ok(),
            _ => None,
        })
    })
}

fn validate_secret(label: &str, secret: &str) -> Result<(), HelperError> {
    if secret.is_empty() {
        return Err(HelperError::InvalidToken(format!("{label} is empty")));
    }
    if secret.len() > MAX_SECRET_BYTES {
        return Err(HelperError::InvalidToken(format!(
            "{label} exceeds {MAX_SECRET_BYTES} bytes"
        )));
    }
    if secret.chars().any(char::is_control) {
        return Err(HelperError::InvalidToken(format!(
            "{label} contains control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_flat_android_shape_without_exposing_secret() {
        let token = parse_token_json(
            r#"{"access_key":"test-access-key","refresh_token":"test-refresh","mid":"42","expires_at":"123"}"#,
        )
        .unwrap();
        assert_eq!(token.access_key(), "test-access-key");
        assert_eq!(token.mid(), Some("42"));
        assert_eq!(token.expires_at(), Some(123));
        assert!(token.has_refresh_token());
        assert!(!format!("{token:?}").contains("test-access-key"));
    }

    #[test]
    fn converts_relative_expiry_and_keeps_fast_login_token() {
        let token = parse_token_json(
            r#"{"access_token":"test-token","expire_in":3600,"fast_login_token":"fast"}"#,
        )
        .unwrap();
        assert!(token.expires_at().unwrap() > now_ts());
        assert_eq!(token.fast_login_token.as_deref(), Some("fast"));
    }

    #[test]
    fn imports_wrapped_shape_and_access_token_alias() {
        let token = parse_token_json(r#"{"data":{"access_token":"test-token","uid":7}}"#).unwrap();
        assert_eq!(token.access_key(), "test-token");
        assert_eq!(token.mid(), Some("7"));
    }

    #[test]
    fn rejects_missing_or_invalid_access_key() {
        assert!(parse_token_json(r#"{"mid":1}"#).is_err());
        assert!(parse_token_json(r#"{"access_key":"a\nsecret"}"#).is_err());
    }

    #[test]
    fn store_status_does_not_include_secret() {
        let mut store = TokenStore::default();
        assert!(!store.status().configured);
        store
            .import_json(r#"{"access_key":"test-access-key"}"#)
            .unwrap();
        assert_eq!(store.status().configured, true);
        store.clear();
        assert!(!store.status().configured);
    }
}
