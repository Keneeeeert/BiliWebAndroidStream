//! Android PlayViewUnite protobuf/gRPC client and normalized DASH response.

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use md5::{Digest, Md5};
use prost::Message;
use reqwest::blocking::Client as HttpClient;
use serde::Serialize;

use crate::{error::HelperError, token::AndroidToken};

const DEFAULT_FNVAL: u32 = 4048;
const DEFAULT_BUILD: u32 = 7_760_700;
const DEFAULT_VERSION_NAME: &str = "7.76.0";
const GRPC_PATH: &str = "/bilibili.app.playerunite.v1.Player/PlayViewUnite";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PlayRequest {
    pub bvid: String,
    #[serde(default)]
    pub aid: Option<u64>,
    pub cid: u64,
    #[serde(default)]
    pub qn: Option<u32>,
    #[serde(default)]
    pub fnval: Option<u32>,
    #[serde(default)]
    pub fourk: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackResponse {
    pub bvid: String,
    pub cid: u64,
    pub quality: u32,
    pub duration_ms: u64,
    pub streams: Vec<DashStream>,
    pub audio: Vec<DashAudio>,
    pub source: PlaybackSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackSource {
    AndroidGrpc,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashStream {
    pub quality: u32,
    pub mime_type: String,
    pub codecs: Option<String>,
    pub width: u32,
    pub height: u32,
    pub bandwidth: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub need_vip: bool,
    pub vip_free: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashAudio {
    pub id: u32,
    pub mime_type: String,
    pub bandwidth: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
}

pub trait PlaybackClient {
    fn resolve_playurl(
        &self,
        request: &PlayRequest,
        token: &AndroidToken,
    ) -> Result<PlaybackResponse, HelperError>;
}

#[derive(Debug, Clone)]
pub struct GrpcPlaybackClient {
    endpoint: String,
}

impl Default for GrpcPlaybackClient {
    fn default() -> Self {
        Self {
            endpoint: "https://grpc.biliapi.net".to_owned(),
        }
    }
}

impl GrpcPlaybackClient {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl PlaybackClient for GrpcPlaybackClient {
    fn resolve_playurl(
        &self,
        request: &PlayRequest,
        token: &AndroidToken,
    ) -> Result<PlaybackResponse, HelperError> {
        let protobuf = PlayViewUniteReq::from_request(request).encode_to_vec();
        let metadata = Metadata::for_token(token).encode_to_vec();
        let device = Device::for_token(token).encode_to_vec();
        let network = Network { network_type: 1 }.encode_to_vec();
        let response = HttpClient::builder()
            .http2_adaptive_window(true)
            .build()
            .map_err(|e| HelperError::Network(e.to_string()))?
            .post(format!("{}{}", self.endpoint, GRPC_PATH))
            .header("content-type", "application/grpc")
            .header("grpc-encoding", "identity")
            .header(
                "authorization",
                format!("identify_v1 {}", token.access_key()),
            )
            .header("x-bili-metadata-bin", BASE64.encode(metadata))
            .header("x-bili-device-bin", BASE64.encode(device))
            .header("x-bili-network-bin", BASE64.encode(network))
            .body(grpc_frame(&protobuf))
            .send()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let status = response.status();
        let body = response
            .bytes()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        if !status.is_success() {
            return Err(HelperError::Network(format!("gRPC HTTP status {status}")));
        }
        let message = parse_grpc_frame(&body)?;
        let reply = PlayViewUniteReply::decode(message)
            .map_err(|e| HelperError::Protobuf(e.to_string()))?;
        normalize_reply(request, reply)
    }
}

fn normalize_reply(
    request: &PlayRequest,
    reply: PlayViewUniteReply,
) -> Result<PlaybackResponse, HelperError> {
    let vod = reply
        .vod_info
        .ok_or_else(|| HelperError::PlaybackResponse("response has no vod_info".into()))?;
    let mut streams = Vec::new();
    for stream in vod.stream_list {
        let (Some(info), Some(video)) = (stream.stream_info, stream.dash_video) else {
            continue;
        };
        if video.base_url.is_empty() {
            continue;
        }
        streams.push(DashStream {
            quality: info.quality,
            mime_type: "video/mp4".into(),
            codecs: Some(web_codec(video.codecid, video.width, video.height).into()),
            width: video.width,
            height: video.height,
            bandwidth: video.bandwidth,
            base_url: video.base_url,
            backup_urls: video.backup_url,
            // Match the Android patch: retain the server-provided DASH URL
            // while removing the local client quality gate.
            need_vip: false,
            vip_free: true,
        });
    }
    let audio = vod
        .dash_audio
        .into_iter()
        .filter(|x| !x.base_url.is_empty())
        .map(|x| DashAudio {
            id: x.id,
            mime_type: "audio/mp4".into(),
            bandwidth: x.bandwidth,
            base_url: x.base_url,
            backup_urls: x.backup_url,
        })
        .collect();
    if streams.is_empty() {
        return Err(HelperError::PlaybackResponse(
            "response has no DASH video URL for this account and quality".into(),
        ));
    }
    Ok(PlaybackResponse {
        bvid: request.bvid.clone(),
        cid: request.cid,
        quality: vod.quality,
        duration_ms: vod.timelength,
        streams,
        audio,
        source: PlaybackSource::AndroidGrpc,
    })
}

fn web_codec(codecid: u32, width: u32, height: u32) -> &'static str {
    match codecid {
        // Bilibili codecid 7 is AVC. The web player expects an RFC 6381
        // codec string; exposing `codecid-7` makes Firefox reject the DASH
        // representation before the requested quality can be rendered.
        7 if width >= 1920 || height >= 1080 => "avc1.640028",
        7 if width >= 1280 || height >= 720 => "avc1.64001f",
        7 if width >= 854 || height >= 480 => "avc1.4d401f",
        7 => "avc1.4d401e",
        12 => "hev1.1.6.L120.90",
        13 => "av01.0.08M.08",
        _ => "avc1.640028",
    }
}

fn grpc_frame(message: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(message.len() + 5);
    frame.push(0);
    frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
    frame.extend_from_slice(message);
    frame
}

fn parse_grpc_frame(body: &[u8]) -> Result<&[u8], HelperError> {
    if body.len() < 5 {
        return Err(HelperError::Protobuf("short gRPC frame".into()));
    }
    let length = u32::from_be_bytes([body[1], body[2], body[3], body[4]]) as usize;
    if body[0] != 0 || body.len() < 5 + length {
        return Err(HelperError::Protobuf("invalid gRPC frame".into()));
    }
    Ok(&body[5..5 + length])
}

#[derive(Clone, PartialEq, Message)]
struct PlayViewUniteReq {
    #[prost(message, optional, tag = "1")]
    vod: Option<VideoVod>,
    #[prost(string, tag = "2")]
    spmid: String,
    #[prost(map = "string, string", tag = "4")]
    extra_content: HashMap<String, String>,
    #[prost(string, tag = "5")]
    bvid: String,
    #[prost(string, tag = "8")]
    from_scene: String,
    #[prost(string, tag = "3")]
    from_spmid: String,
}

impl PlayViewUniteReq {
    fn from_request(request: &PlayRequest) -> Self {
        Self {
            vod: Some(VideoVod {
                aid: request.aid.unwrap_or(0),
                cid: request.cid,
                qn: request.qn.unwrap_or(112),
                fnver: 0,
                fnval: request.fnval.unwrap_or(DEFAULT_FNVAL),
                download: 0,
                force_host: 2,
                fourk: request.fourk.unwrap_or(true),
                prefer_codec_type: 0,
                voice_balance: 0,
                is_need_trial: true,
            }),
            spmid: "united.player-video-detail.0.0".into(),
            extra_content: HashMap::new(),
            bvid: request.bvid.clone(),
            from_scene: "normal".into(),
            from_spmid: "0.0.0.0".into(),
        }
    }
}

#[derive(Clone, PartialEq, Message)]
struct VideoVod {
    #[prost(uint64, tag = "1")]
    aid: u64,
    #[prost(uint64, tag = "2")]
    cid: u64,
    #[prost(uint32, tag = "6")]
    download: u32,
    #[prost(uint32, tag = "5")]
    fnval: u32,
    #[prost(uint32, tag = "4")]
    fnver: u32,
    #[prost(uint32, tag = "7")]
    force_host: u32,
    #[prost(bool, tag = "8")]
    fourk: bool,
    #[prost(bool, tag = "11")]
    is_need_trial: bool,
    #[prost(uint32, tag = "9")]
    prefer_codec_type: u32,
    #[prost(uint32, tag = "3")]
    qn: u32,
    #[prost(uint32, tag = "10")]
    voice_balance: u32,
}

#[derive(Clone, PartialEq, Message)]
struct PlayViewUniteReply {
    #[prost(message, optional, tag = "1")]
    vod_info: Option<VodInfo>,
}
#[derive(Clone, PartialEq, Message)]
struct VodInfo {
    #[prost(uint32, tag = "1")]
    quality: u32,
    #[prost(uint64, tag = "3")]
    timelength: u64,
    #[prost(message, repeated, tag = "5")]
    stream_list: Vec<Stream>,
    #[prost(message, repeated, tag = "6")]
    dash_audio: Vec<DashItem>,
}
#[derive(Clone, PartialEq, Message)]
struct Stream {
    #[prost(message, optional, tag = "1")]
    stream_info: Option<StreamInfo>,
    #[prost(message, optional, tag = "2")]
    dash_video: Option<DashVideo>,
}
#[derive(Clone, PartialEq, Message)]
struct StreamInfo {
    #[prost(uint32, tag = "1")]
    quality: u32,
    #[prost(string, tag = "2")]
    format: String,
    #[prost(string, tag = "3")]
    description: String,
    #[prost(bool, tag = "6")]
    need_vip: bool,
    #[prost(bool, tag = "7")]
    need_login: bool,
    #[prost(bool, tag = "8")]
    intact: bool,
    #[prost(string, tag = "11")]
    new_description: String,
    #[prost(string, tag = "12")]
    display_desc: String,
    #[prost(string, tag = "13")]
    superscript: String,
    #[prost(bool, tag = "14")]
    vip_free: bool,
}
#[derive(Clone, PartialEq, Message)]
struct DashVideo {
    #[prost(string, tag = "1")]
    base_url: String,
    #[prost(string, repeated, tag = "2")]
    backup_url: Vec<String>,
    #[prost(uint32, tag = "3")]
    bandwidth: u32,
    #[prost(uint32, tag = "4")]
    codecid: u32,
    #[prost(uint32, tag = "7")]
    audio_id: u32,
    #[prost(string, tag = "9")]
    frame_rate: String,
    #[prost(uint32, tag = "10")]
    width: u32,
    #[prost(uint32, tag = "11")]
    height: u32,
}
#[derive(Clone, PartialEq, Message)]
struct DashItem {
    #[prost(uint32, tag = "1")]
    id: u32,
    #[prost(string, tag = "2")]
    base_url: String,
    #[prost(string, repeated, tag = "3")]
    backup_url: Vec<String>,
    #[prost(uint32, tag = "4")]
    bandwidth: u32,
}
#[derive(Clone, PartialEq, Message)]
struct Metadata {
    #[prost(string, tag = "1")]
    access_key: String,
    #[prost(string, tag = "2")]
    mobi_app: String,
    #[prost(string, tag = "3")]
    device: String,
    #[prost(uint32, tag = "4")]
    build: u32,
    #[prost(string, tag = "5")]
    channel: String,
    #[prost(string, tag = "6")]
    buvid: String,
    #[prost(string, tag = "7")]
    platform: String,
}
#[derive(Clone, PartialEq, Message)]
struct Device {
    #[prost(uint32, tag = "1")]
    app_id: u32,
    #[prost(uint32, tag = "2")]
    build: u32,
    #[prost(string, tag = "3")]
    buvid: String,
    #[prost(string, tag = "4")]
    mobi_app: String,
    #[prost(string, tag = "5")]
    platform: String,
    #[prost(string, tag = "6")]
    device: String,
    #[prost(string, tag = "7")]
    channel: String,
    #[prost(string, tag = "8")]
    brand: String,
    #[prost(string, tag = "9")]
    model: String,
    #[prost(string, tag = "10")]
    osver: String,
    #[prost(string, tag = "13")]
    version_name: String,
}
#[derive(Clone, PartialEq, Message)]
struct Network {
    #[prost(uint32, tag = "1")]
    network_type: u32,
}

impl Metadata {
    fn for_token(token: &AndroidToken) -> Self {
        let buvid = effective_buvid(token);
        Self {
            access_key: token.access_key().into(),
            mobi_app: "android".into(),
            device: String::new(),
            build: DEFAULT_BUILD,
            channel: "master".into(),
            buvid,
            platform: "android".into(),
        }
    }
}
impl Device {
    fn for_token(token: &AndroidToken) -> Self {
        let buvid = effective_buvid(token);
        Self {
            app_id: 1,
            build: DEFAULT_BUILD,
            buvid,
            mobi_app: "android".into(),
            platform: "android".into(),
            device: String::new(),
            channel: "master".into(),
            brand: "unknown".into(),
            model: "unknown".into(),
            osver: "unknown".into(),
            version_name: DEFAULT_VERSION_NAME.into(),
        }
    }
}

fn effective_buvid(token: &AndroidToken) -> String {
    if let Some(buvid) = token.buvid() {
        return buvid.to_owned();
    }
    // The Android request always carries a non-empty buvid. Keep a stable
    // local fallback for QR-imported tokens that do not include web cookies;
    // an empty buvid causes the CDN URL returned by PlayViewUnite to be
    // rejected even though the gRPC response itself is successful.
    let digest = Md5::digest(b"bili-web-android-stream");
    let hex = format!("{:x}", digest);
    format!(
        "{}-{}-{}-{}-{}infoc",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
