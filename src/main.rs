mod error;
mod native_messaging;
mod playback;
mod protocol;
mod qr;
mod token;
mod uninstall;

use std::io::{self, BufReader, BufWriter};

use native_messaging::{read_frame, write_frame};
use playback::{GrpcPlaybackClient, PlayRequest, PlaybackClient};
use protocol::{Command, RequestEnvelope, ResponseBody, ResponseEnvelope};
use qr::{QrClient, WebCookies};
use token::TokenStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut tokens = TokenStore::open_default();
    let client = GrpcPlaybackClient::default();
    let mut qr = QrClient::default();

    while let Some(frame) = read_frame(&mut reader)? {
        let response = match serde_json::from_slice::<RequestEnvelope>(&frame) {
            Ok(request) => handle(request, &mut tokens, &client, &mut qr),
            Err(error) => ResponseEnvelope::error(None, "invalid_request", error.to_string()),
        };
        let encoded = serde_json::to_vec(&response)?;
        write_frame(&mut writer, &encoded)?;
    }
    Ok(())
}

fn handle(
    request: RequestEnvelope,
    tokens: &mut TokenStore,
    client: &GrpcPlaybackClient,
    qr: &mut QrClient,
) -> ResponseEnvelope {
    let request_id = request.request_id;
    let result = match request.command {
        Command::Ping => Ok(ResponseBody::Pong),
        Command::Info => Ok(ResponseBody::Info {
            name: "com.biliwebandroidstream.helper".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: 1,
            target: target_name().into(),
            capabilities: vec![
                "token".into(),
                "qr".into(),
                "playback".into(),
                "refresh".into(),
                "uninstall".into(),
            ],
        }),
        Command::Status => {
            let status = tokens.status();
            Ok(ResponseBody::Status {
                configured: status.configured,
                has_refresh_token: status.has_refresh_token,
                expires_at: status.expires_at,
            })
        }
        Command::SetToken { token_json } => {
            tokens
                .import_json(&token_json)
                .map(|status| ResponseBody::TokenSet {
                    status: status.into(),
                })
        }
        Command::ClearToken => {
            tokens.clear();
            Ok(ResponseBody::TokenCleared)
        }
        Command::SetBuvid { buvid } => {
            tokens
                .set_buvid(buvid)
                .map(|status| ResponseBody::TokenSet {
                    status: status.into(),
                })
        }
        Command::QrStart => qr.start().map(|session| ResponseBody::QrStarted {
            url: session.url,
            auth_code: session.auth_code,
            qr_svg: session.qr_svg,
        }),
        Command::QrPoll => qr.poll(tokens).map(|result| ResponseBody::QrPolled {
            state: format!("{:?}", result.state).to_lowercase(),
            message: result.message,
            status: result.token_status.map(Into::into),
        }),
        Command::RefreshToken => qr.refresh(tokens).map(|status| ResponseBody::TokenSet {
            status: status.into(),
        }),
        Command::PlaybackAuth => tokens
            .token()
            .map(|token| ResponseBody::PlaybackAuth {
                authorization: format!("identify_v1 {}", token.access_key()),
            })
            .ok_or(crate::error::HelperError::MissingToken),
        Command::Uninstall => {
            // The response is written by the main loop after this returns; the
            // binary's own removal is either an unlink (Unix, harmless while
            // running) or a delayed detached cleanup (Windows).
            let report = crate::uninstall::run();
            Ok(ResponseBody::Uninstalled {
                manifest_removed: report.manifest_removed,
                token_removed: report.token_removed,
                binary_removed: report.binary_removed,
            })
        }
        Command::WebCookieLogin {
            sessdata,
            dede_user_id,
            bili_jct,
        } => qr
            .exchange_web_cookies(
                WebCookies {
                    sessdata,
                    dede_user_id,
                    bili_jct,
                },
                tokens,
            )
            .map(|result| ResponseBody::QrPolled {
                state: format!("{:?}", result.state).to_lowercase(),
                message: result.message,
                status: result.token_status.map(Into::into),
            }),
        Command::ResolvePlayUrl {
            bvid,
            aid,
            cid,
            qn,
            fnval,
            fourk,
        } => match tokens.token() {
            Some(token) => {
                let needs_refresh = token.needs_refresh_at(now_ts()) && token.has_refresh_token();
                let refresh = if needs_refresh {
                    qr.refresh(tokens).map(|_| ())
                } else {
                    Ok(())
                };
                refresh.and_then(|_| {
                    let token = tokens
                        .token()
                        .ok_or(crate::error::HelperError::MissingToken)?;
                    let request = PlayRequest {
                        bvid,
                        aid,
                        cid,
                        qn,
                        fnval,
                        fourk,
                    };
                    client
                        .resolve_playurl(&request, token)
                        .map(|response| ResponseBody::PlayUrl { response })
                })
            }
            None => Err(crate::error::HelperError::MissingToken),
        },
    };

    match result {
        Ok(body) => ResponseEnvelope::success(request_id, body),
        Err(error) => ResponseEnvelope::error(request_id, error_code(&error), error.to_string()),
    }
}

fn target_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn error_code(error: &crate::error::HelperError) -> &'static str {
    match error {
        crate::error::HelperError::InvalidToken(_) => "invalid_token",
        crate::error::HelperError::MissingToken => "missing_token",
        crate::error::HelperError::Network(_) => "network_error",
        crate::error::HelperError::Protobuf(_) => "protobuf_error",
        crate::error::HelperError::PlaybackResponse(_) => "playback_response_error",
        crate::error::HelperError::Qr(_) => "qr_error",
        crate::error::HelperError::FrameTooLarge { .. } => "frame_too_large",
        crate::error::HelperError::TruncatedFrame { .. } => "truncated_frame",
        crate::error::HelperError::InvalidUtf8(_) => "invalid_utf8",
        crate::error::HelperError::Json(_) => "invalid_json",
        crate::error::HelperError::Io(_) => "io_error",
    }
}
