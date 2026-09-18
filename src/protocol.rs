//! JSON messages exchanged with the Firefox extension.

use serde::{Deserialize, Serialize};

use crate::token::TokenStatus;

#[derive(Debug, Deserialize)]
pub struct RequestEnvelope {
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Info,
    Status,
    SetToken {
        token_json: String,
    },
    ClearToken,
    SetBuvid {
        buvid: String,
    },
    QrStart,
    QrPoll,
    RefreshToken,
    PlaybackAuth,
    WebCookieLogin {
        sessdata: String,
        dede_user_id: String,
        bili_jct: String,
    },
    ResolvePlayUrl {
        bvid: String,
        #[serde(default)]
        aid: Option<u64>,
        cid: u64,
        #[serde(default)]
        qn: Option<u32>,
        #[serde(default)]
        fnval: Option<u32>,
        #[serde(default)]
        fourk: Option<bool>,
    },
}

#[derive(Debug, Serialize)]
pub struct ResponseEnvelope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(flatten)]
    pub body: ResponseBody,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseBody {
    Pong,
    Info {
        name: String,
        version: String,
        protocol_version: u32,
        target: String,
        capabilities: Vec<String>,
    },
    Status {
        configured: bool,
        has_refresh_token: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<u64>,
    },
    TokenSet {
        status: TokenStatusResponse,
    },
    TokenCleared,
    PlaybackAuth {
        authorization: String,
    },
    QrStarted {
        url: String,
        auth_code: String,
        qr_svg: String,
    },
    QrPolled {
        state: String,
        message: String,
        status: Option<TokenStatusResponse>,
    },
    PlayUrl {
        response: crate::playback::PlaybackResponse,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Serialize)]
pub struct TokenStatusResponse {
    pub configured: bool,
    pub has_refresh_token: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

impl From<TokenStatus> for TokenStatusResponse {
    fn from(status: TokenStatus) -> Self {
        Self {
            configured: status.configured,
            has_refresh_token: status.has_refresh_token,
            expires_at: status.expires_at,
        }
    }
}

impl ResponseEnvelope {
    pub fn success(request_id: Option<String>, body: ResponseBody) -> Self {
        Self { request_id, body }
    }

    pub fn error(
        request_id: Option<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id,
            body: ResponseBody::Error {
                code: code.into(),
                message: message.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_play_request_with_defaults() {
        let envelope: RequestEnvelope = serde_json::from_str(
            r#"{"request_id":"1","type":"resolve_play_url","bvid":"BV1test","cid":123}"#,
        )
        .unwrap();
        assert_eq!(envelope.request_id.as_deref(), Some("1"));
        match envelope.command {
            Command::ResolvePlayUrl { bvid, cid, qn, .. } => {
                assert_eq!(bvid, "BV1test");
                assert_eq!(cid, 123);
                assert_eq!(qn, None);
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn serializes_error_without_secret_fields() {
        let output = serde_json::to_string(&ResponseEnvelope::error(
            Some("1".into()),
            "invalid_token",
            "token is invalid",
        ))
        .unwrap();
        assert!(output.contains("invalid_token"));
        assert!(!output.contains("access_key"));
    }
}
