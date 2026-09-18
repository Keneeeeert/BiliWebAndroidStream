//! Experimental TV/APP QR login flow.
//!
//! This uses the currently observed TV client parameters only as an
//! experiment. It must be validated against the playback entitlement before
//! becoming a default login path.

use std::time::{SystemTime, UNIX_EPOCH};

use md5::{Digest, Md5};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    error::HelperError,
    token::{TokenStatus, TokenStore},
};

const APP_KEY: &str = "4409e2ce8ffd12b8";
const APP_SECRET: &str = "59b43e04ad6965f34319062b478f83dd";
const WEB_EXCHANGE_APP_KEY: &str = "783bbb7264451d82";
const WEB_EXCHANGE_APP_SECRET: &str = "2653583c8873dea268ab9386918b1d65";
const LOCAL_ID: &str = "0";
const MOBI_APP: &str = "android";
const BUILD: &str = "7760700";
const DEVICE: &str = "phone";
const DEVICE_PLATFORM: &str = "android";
const USER_AGENT: &str = "Mozilla/5.0 BiliDroid/7.76.0";
const QR_AUTH_ENDPOINT: &str = "https://passport.bilibili.com/x/passport-tv-login/qrcode/auth_code";
const QR_POLL_ENDPOINT: &str = "https://passport.bilibili.com/x/passport-tv-login/qrcode/poll";
const REFRESH_ENDPOINT: &str =
    "https://passport.bilibili.com/x/passport-login/oauth2/refresh_token";

#[derive(Clone)]
pub struct QrSession {
    pub auth_code: String,
    pub url: String,
    pub qr_svg: String,
    pub created_at: u64,
    app_key: String,
    app_secret: String,
}

impl std::fmt::Debug for QrSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QrSession")
            .field("auth_code", &"<redacted>")
            .field("url", &self.url)
            .field("qr_svg", &"<generated>")
            .field("created_at", &self.created_at)
            .field("app_key", &self.app_key)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrState {
    Pending,
    Expired,
    Authorized,
}

#[derive(Debug, Clone)]
pub struct QrPollResult {
    pub state: QrState,
    pub message: String,
    pub token_status: Option<TokenStatus>,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    code: i32,
    message: String,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct AuthCodeData {
    auth_code: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct PollData {
    mid: Option<Value>,
    access_token: Option<String>,
    access_key: Option<String>,
    refresh_token: Option<String>,
    fast_login_token: Option<String>,
    expires: Option<u64>,
    expires_in: Option<u64>,
    expires_at: Option<u64>,
    token_info: Option<TokenInfo>,
}

#[derive(Debug, Deserialize)]
struct TokenInfo {
    mid: Option<Value>,
    access_token: Option<String>,
    access_key: Option<String>,
    refresh_token: Option<String>,
    fast_login_token: Option<String>,
    expires: Option<u64>,
    expires_in: Option<u64>,
    expires_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ServerTimestamp {
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct WebCookieStatus {
    refresh: bool,
}

pub struct QrClient {
    http: Client,
    session: Option<QrSession>,
}

impl Default for QrClient {
    fn default() -> Self {
        Self {
            http: Client::new(),
            session: None,
        }
    }
}

impl QrClient {
    pub fn start(&mut self) -> Result<QrSession, HelperError> {
        self.start_with_secret(APP_KEY, APP_SECRET, USER_AGENT)
    }

    fn start_with_secret(
        &mut self,
        app_key: &str,
        app_secret: &str,
        user_agent: &str,
    ) -> Result<QrSession, HelperError> {
        let ts = now_ts();
        let params = vec![
            ("appkey", app_key.to_owned()),
            ("local_id", LOCAL_ID.to_owned()),
            ("mobi_app", MOBI_APP.to_owned()),
            ("ts", ts.to_string()),
        ];
        let response = self
            .http
            .post(QR_AUTH_ENDPOINT)
            .header("User-Agent", user_agent)
            .form(&signed_params_with_secret(&params, app_secret))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<AuthCodeData> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() || body.code != 0 {
            return Err(HelperError::Qr(format!(
                "QR generate failed: {} {}",
                body.code, body.message
            )));
        }
        let data = body
            .data
            .ok_or_else(|| HelperError::Qr("QR response has no data".into()))?;
        let qr_svg = make_qr_svg(&data.url)?;
        let session = QrSession {
            auth_code: data.auth_code,
            url: data.url,
            qr_svg,
            created_at: ts,
            app_key: app_key.to_owned(),
            app_secret: app_secret.to_owned(),
        };
        self.session = Some(session.clone());
        Ok(session)
    }

    pub fn poll(&mut self, tokens: &mut TokenStore) -> Result<QrPollResult, HelperError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| HelperError::Qr("no QR session".into()))?;
        // Passport's TV endpoint uses Unix seconds. Milliseconds are accepted
        // by some deployments as a pending response, but are rejected after
        // authorization and make the login flow look permanently pending.
        let ts = now_ts();
        let params = vec![
            ("appkey", session.app_key.clone()),
            ("auth_code", session.auth_code.clone()),
            ("local_id", LOCAL_ID.to_owned()),
            ("ts", ts.to_string()),
        ];
        let response = self
            .http
            .post(QR_POLL_ENDPOINT)
            .header("User-Agent", USER_AGENT)
            .form(&signed_params_with_secret(&params, &session.app_secret))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<PollData> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(HelperError::Qr(format!(
                "QR poll HTTP status {}: {}",
                status, body.message
            )));
        }
        self.handle_poll_body(body, tokens)
    }

    /// Exchange the current web session for an APP token through the TV/APP
    /// authorization bridge. Cookies stay in memory and are sent only to the
    /// official Passport endpoint.
    pub fn exchange_web_cookies(
        &mut self,
        cookies: WebCookies,
        tokens: &mut TokenStore,
    ) -> Result<QrPollResult, HelperError> {
        if cookies.sessdata.is_empty()
            || cookies.dede_user_id.is_empty()
            || cookies.bili_jct.is_empty()
        {
            return Err(HelperError::Qr(
                "current web session is missing SESSDATA, DedeUserID, or bili_jct".into(),
            ));
        }
        for (name, value) in [
            ("SESSDATA", &cookies.sessdata),
            ("DedeUserID", &cookies.dede_user_id),
            ("bili_jct", &cookies.bili_jct),
        ] {
            if value.len() > 4096 || value.chars().any(char::is_control) {
                return Err(HelperError::Qr(format!("invalid {name} cookie value")));
            }
        }
        self.ensure_web_cookie_is_current(&cookies)?;
        let ts = now_ts();
        let params = vec![
            ("appkey", WEB_EXCHANGE_APP_KEY.to_owned()),
            ("local_id", LOCAL_ID.to_owned()),
            ("ts", ts.to_string()),
        ];
        let response = self
            .http
            .post(QR_AUTH_ENDPOINT)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&signed_params_with_secret(&params, WEB_EXCHANGE_APP_SECRET))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<AuthCodeData> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() || body.code != 0 {
            return Err(HelperError::Qr(format!(
                "web session auth_code failed: {} {}",
                body.code, body.message
            )));
        }
        let data = body
            .data
            .ok_or_else(|| HelperError::Qr("web session auth_code has no data".into()))?;
        let auth_code = data.auth_code;
        let cookie_header = format!(
            "DedeUserID={}; SESSDATA={}; bili_jct={}",
            cookies.dede_user_id, cookies.sessdata, cookies.bili_jct
        );
        let confirm = self
            .http
            .post("https://passport.bilibili.com/x/passport-tv-login/h5/qrcode/confirm")
            .header("User-Agent", "BiliDroid/7.82.0")
            .header("Cookie", cookie_header)
            .form(&[
                ("auth_code", auth_code.as_str()),
                ("build", "7082000"),
                ("csrf", cookies.bili_jct.as_str()),
            ])
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let confirm_status = confirm.status();
        let confirm_body: Envelope<serde_json::Value> = confirm
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !confirm_status.is_success() || confirm_body.code != 0 {
            return Err(HelperError::Qr(format!(
                "web session confirm failed: {} {}",
                confirm_body.code, confirm_body.message
            )));
        }
        let qr_svg = make_qr_svg(&data.url)?;
        self.session = Some(QrSession {
            auth_code,
            url: data.url,
            qr_svg,
            created_at: ts,
            app_key: WEB_EXCHANGE_APP_KEY.to_owned(),
            app_secret: WEB_EXCHANGE_APP_SECRET.to_owned(),
        });
        self.poll_with_secret(tokens)
    }

    fn ensure_web_cookie_is_current(&self, cookies: &WebCookies) -> Result<(), HelperError> {
        let response = self
            .http
            .get("https://passport.bilibili.com/x/passport-login/web/cookie/info")
            .header(
                "Cookie",
                format!(
                    "DedeUserID={}; SESSDATA={}; bili_jct={}",
                    cookies.dede_user_id, cookies.sessdata, cookies.bili_jct
                ),
            )
            .header("User-Agent", USER_AGENT)
            .query(&[("csrf", cookies.bili_jct.as_str())])
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<WebCookieStatus> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() || body.code != 0 {
            return Err(HelperError::Qr(format!(
                "web session check failed: {} {}",
                body.code, body.message
            )));
        }
        if body.data.map(|data| data.refresh).unwrap_or(false) {
            return Err(HelperError::Qr(
                "web session needs refresh; sign in again on bilibili.com or use Android QR login"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Refresh an Android access token using the same Passport endpoint and
    /// app signature as the host APK. The server may rotate both tokens, so
    /// the complete response is imported atomically into the TokenStore.
    pub fn refresh(&mut self, tokens: &mut TokenStore) -> Result<TokenStatus, HelperError> {
        let token = tokens
            .token()
            .ok_or_else(|| HelperError::Qr("no token to refresh".into()))?;
        let refresh_token = token
            .refresh_token()
            .ok_or_else(|| HelperError::Qr("stored token has no refresh_token".into()))?;
        let (app_key, app_secret) = match token.app_key() {
            Some(key) if key == WEB_EXCHANGE_APP_KEY => {
                (WEB_EXCHANGE_APP_KEY, WEB_EXCHANGE_APP_SECRET)
            }
            Some(key) if key == APP_KEY => (APP_KEY, APP_SECRET),
            // Tokens imported from Android clients predate this metadata. The
            // Pink/TV profile is the helper's compatible default.
            _ => (APP_KEY, APP_SECRET),
        };
        let sts = self.server_timestamp(app_key, app_secret).unwrap_or(-1);
        let params = vec![
            ("access_key", token.access_key().to_owned()),
            ("refresh_token", refresh_token.to_owned()),
            // The Android client sends the server timestamp as `sts`. The
            // endpoint accepts the local clock when the timestamp service is
            // unavailable, which keeps this bridge usable offline from the
            // extra timestamp request.
            ("sts", sts.to_string()),
            ("appkey", app_key.to_owned()),
            ("local_id", LOCAL_ID.to_owned()),
            ("mobi_app", MOBI_APP.to_owned()),
            ("build", BUILD.to_owned()),
            ("platform", DEVICE_PLATFORM.to_owned()),
            ("device", DEVICE.to_owned()),
        ];
        let response = self
            .http
            .post(REFRESH_ENDPOINT)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&signed_params_with_secret(&params, app_secret))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<PollData> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(HelperError::Qr(format!(
                "token refresh HTTP status {}: {}",
                status, body.message
            )));
        }
        if body.code != 0 {
            return Err(HelperError::Qr(format!(
                "token refresh failed: {} {}",
                body.code, body.message
            )));
        }
        let token_json = token_json_from_poll(body.data, body.message, Some(app_key))?;
        tokens.import_json(&token_json)
    }

    fn server_timestamp(&self, app_key: &str, app_secret: &str) -> Result<i64, HelperError> {
        let params = vec![
            ("appkey", app_key.to_owned()),
            ("local_id", LOCAL_ID.to_owned()),
            ("mobi_app", MOBI_APP.to_owned()),
            ("ts", now_ts().to_string()),
        ];
        let response = self
            .http
            .get("https://passport.bilibili.com/x/passport-login/timestamp")
            .header("User-Agent", USER_AGENT)
            .query(&signed_params_with_secret(&params, app_secret))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<ServerTimestamp> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() || body.code != 0 {
            return Err(HelperError::Qr(format!(
                "server timestamp failed: {} {}",
                body.code, body.message
            )));
        }
        body.data
            .map(|value| value.timestamp)
            .ok_or_else(|| HelperError::Qr("server timestamp response has no data".into()))
    }

    pub fn clear(&mut self) {
        self.session = None;
    }

    fn poll_with_secret(&mut self, tokens: &mut TokenStore) -> Result<QrPollResult, HelperError> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| HelperError::Qr("no QR session".into()))?;
        let params = vec![
            ("appkey", session.app_key.clone()),
            ("auth_code", session.auth_code.clone()),
            ("local_id", LOCAL_ID.to_owned()),
            ("ts", now_ts().to_string()),
        ];
        let response = self
            .http
            .post(QR_POLL_ENDPOINT)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&signed_params_with_secret(&params, &session.app_secret))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body: Envelope<PollData> = response
            .json()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(HelperError::Qr(format!(
                "QR poll HTTP status {}: {}",
                status, body.message
            )));
        }
        self.handle_poll_body(body, tokens)
    }

    fn handle_poll_body(
        &mut self,
        body: Envelope<PollData>,
        tokens: &mut TokenStore,
    ) -> Result<QrPollResult, HelperError> {
        match body.code {
            0 => {
                let app_key = self
                    .session
                    .as_ref()
                    .map(|session| session.app_key.as_str());
                let token_json = token_json_from_poll(body.data, body.message.clone(), app_key)?;
                let status = tokens.import_json(&token_json)?;
                self.session = None;
                Ok(QrPollResult {
                    state: QrState::Authorized,
                    message: body.message,
                    token_status: Some(status),
                })
            }
            86038 => Ok(QrPollResult {
                state: QrState::Expired,
                message: body.message,
                token_status: None,
            }),
            86039 | 86090 | 86101 => Ok(QrPollResult {
                state: QrState::Pending,
                message: body.message,
                token_status: None,
            }),
            code => Err(HelperError::Qr(format!(
                "QR poll failed: {code} {}",
                body.message
            ))),
        }
    }
}

fn token_json_from_poll(
    data: Option<PollData>,
    context: String,
    app_key: Option<&str>,
) -> Result<String, HelperError> {
    let data =
        data.ok_or_else(|| HelperError::Qr(format!("token response has no data: {context}")))?;
    let nested = data.token_info;
    let access_token = data
        .access_token
        .or(data.access_key)
        .or_else(|| nested.as_ref().and_then(|token| token.access_token.clone()))
        .or_else(|| nested.as_ref().and_then(|token| token.access_key.clone()))
        .ok_or_else(|| HelperError::Qr("token response has no access_token".into()))?;
    let refresh_token = data.refresh_token.or_else(|| {
        nested
            .as_ref()
            .and_then(|token| token.refresh_token.clone())
    });
    let fast_login_token = data.fast_login_token.or_else(|| {
        nested
            .as_ref()
            .and_then(|token| token.fast_login_token.clone())
    });
    let mid = data
        .mid
        .or_else(|| nested.as_ref().and_then(|token| token.mid.clone()));
    let expires_at = data
        .expires
        .or_else(|| nested.as_ref().and_then(|token| token.expires))
        .or(data.expires_at)
        .or_else(|| nested.as_ref().and_then(|token| token.expires_at))
        .or_else(|| {
            data.expires_in
                .or_else(|| nested.as_ref().and_then(|token| token.expires_in))
                .map(|seconds| now_ts().saturating_add(seconds))
        });
    Ok(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "fast_login_token": fast_login_token,
        "app_key": app_key,
        "mid": mid,
        "expires_at": expires_at,
    })
    .to_string())
}

#[derive(Clone, serde::Deserialize)]
pub struct WebCookies {
    pub sessdata: String,
    pub dede_user_id: String,
    pub bili_jct: String,
}

impl std::fmt::Debug for WebCookies {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebCookies")
            .field("sessdata", &"<redacted>")
            .field("dede_user_id", &"<redacted>")
            .field("bili_jct", &"<redacted>")
            .finish()
    }
}

fn signed_params_with_secret(
    params: &[(impl AsRef<str>, String)],
    secret: &str,
) -> Vec<(String, String)> {
    let mut values: Vec<(String, String)> = params
        .iter()
        .map(|(key, value)| (key.as_ref().to_owned(), value.clone()))
        .collect();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    let canonical = values
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let mut hasher = Md5::new();
    hasher.update(format!("{canonical}{secret}"));
    values.push(("sign".into(), format!("{:x}", hasher.finalize())));
    values
}

fn percent_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
    }
    encoded
}

fn make_qr_svg(value: &str) -> Result<String, HelperError> {
    let code = qrcode::QrCode::new(value.as_bytes())
        .map_err(|error| HelperError::Qr(format!("QR render failed: {error}")))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(360, 360)
        .build())
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_nested_token_info_with_absolute_expiry() {
        let body: Envelope<PollData> = serde_json::from_str(
            r#"{
                "code": 0,
                "message": "OK",
                "data": {
                    "status": 0,
                    "token_info": {
                        "mid": 42,
                        "access_token": "access",
                        "refresh_token": "refresh",
                        "expires": 2000000000,
                        "expires_in": 3600,
                        "fast_login_token": "fast"
                    }
                }
            }"#,
        )
        .unwrap();
        let token = token_json_from_poll(body.data, body.message, Some(APP_KEY)).unwrap();
        let value: Value = serde_json::from_str(&token).unwrap();
        assert_eq!(value["access_token"], "access");
        assert_eq!(value["refresh_token"], "refresh");
        assert_eq!(value["expires_at"], 2_000_000_000u64);
        assert_eq!(value["fast_login_token"], "fast");
        assert_eq!(value["app_key"], APP_KEY);
    }

    #[test]
    fn signs_sorted_params_without_mutating_input() {
        let input = vec![("z", "last".to_owned()), ("a", "first".to_owned())];
        let signed = signed_params_with_secret(&input, "secret");
        assert_eq!(input.len(), 2);
        assert_eq!(signed[0], ("a".to_owned(), "first".to_owned()));
        assert_eq!(signed[1], ("z".to_owned(), "last".to_owned()));
        assert_eq!(signed.len(), 3);
        assert_eq!(signed[2].0, "sign");
    }

    #[test]
    fn percent_encodes_values_for_the_passport_signature() {
        assert_eq!(percent_encode("a+b c/汉"), "a%2Bb%20c%2F%E6%B1%89");
    }
}
