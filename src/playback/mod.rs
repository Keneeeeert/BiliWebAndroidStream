//! Android PlayViewUnite protobuf/gRPC client and normalized DASH response.

use std::{collections::HashMap, io::Read, time::Duration};

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
    pub codecid: u32,
    pub frame_rate: Option<String>,
    pub width: u32,
    pub height: u32,
    pub bandwidth: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub segment_base: Option<SegmentBase>,
    pub need_vip: bool,
    pub vip_free: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashAudio {
    pub id: u32,
    pub mime_type: String,
    pub codecs: Option<String>,
    pub bandwidth: u32,
    pub base_url: String,
    pub backup_urls: Vec<String>,
    pub segment_base: Option<SegmentBase>,
}

/// Byte ranges needed by the web player's custom DASH parser.
///
/// Android's `DashVideo`/`DashItem` protobufs only carry the signed URL. The
/// web player expects the MP4 initialization and SIDX ranges as well, so the
/// helper probes the beginning of each m4s object and fills these in.
#[derive(Debug, Clone, Serialize)]
pub struct SegmentBase {
    pub initialization: String,
    pub index_range: String,
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
        let http = HttpClient::builder()
            .http2_adaptive_window(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| HelperError::Network(e.to_string()))?;
        let response = http
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
        if let Ok(path) = std::env::var("BILI_DUMP_RAW_PLAYURL") {
            // Protocol debugging escape hatch: persist the untouched gRPC
            // payload so wire-format questions can be answered offline.
            let _ = std::fs::write(path, message);
        }
        let reply = PlayViewUniteReply::decode(message)
            .map_err(|e| HelperError::Protobuf(e.to_string()))?;
        let mut normalized = normalize_reply(request, reply)?;
        hydrate_segment_bases(&http, &mut normalized);
        Ok(normalized)
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
            codecs: Some(
                web_codec(video.codecid, video.width, video.height, &video.frame_rate).into(),
            ),
            codecid: video.codecid,
            frame_rate: (!video.frame_rate.is_empty()).then_some(video.frame_rate),
            width: video.width,
            height: video.height,
            bandwidth: video.bandwidth,
            base_url: video.base_url,
            backup_urls: video.backup_url,
            segment_base: None,
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
            // PlayViewUnite's DashItem has no codec string; all currently
            // supported DASH audio tracks are AAC-LC, matching web playurl.
            codecs: Some("mp4a.40.2".into()),
            bandwidth: x.bandwidth,
            base_url: x.base_url,
            backup_urls: x.backup_url,
            segment_base: None,
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

/// Populate the byte ranges expected by Bilibili's web DASH parser.
///
/// `PlayViewUnite` intentionally omits these ranges because Android's player
/// can discover them itself. The web player, however, turns the ranges into
/// `SegmentBase` and will fail to initialize a representation if the object is
/// absent. The URLs are signed by the server, so a small range request is safe
/// and does not require another credential or URL-signing implementation.
fn hydrate_segment_bases(http: &HttpClient, response: &mut PlaybackResponse) {
    // Probe representations concurrently. A sequential pass adds one CDN
    // round-trip per quality and can exceed the page bridge's request timeout
    // before the player ever receives the playurl response.
    std::thread::scope(|scope| {
        let stream_jobs = response
            .streams
            .iter()
            .map(|stream| {
                let client = http.clone();
                let base_url = stream.base_url.clone();
                let backups = stream.backup_urls.clone();
                scope.spawn(move || {
                    probe_segment_base(&client, &base_url).or_else(|| {
                        backups
                            .iter()
                            .find_map(|url| probe_segment_base(&client, url))
                    })
                })
            })
            .collect::<Vec<_>>();
        let audio_jobs = response
            .audio
            .iter()
            .map(|audio| {
                let client = http.clone();
                let base_url = audio.base_url.clone();
                let backups = audio.backup_urls.clone();
                scope.spawn(move || {
                    probe_segment_base(&client, &base_url).or_else(|| {
                        backups
                            .iter()
                            .find_map(|url| probe_segment_base(&client, url))
                    })
                })
            })
            .collect::<Vec<_>>();
        for (stream, job) in response.streams.iter_mut().zip(stream_jobs) {
            stream.segment_base = job.join().ok().flatten();
        }
        for (audio, job) in response.audio.iter_mut().zip(audio_jobs) {
            audio.segment_base = job.join().ok().flatten();
        }
    });
}

fn probe_segment_base(http: &HttpClient, url: &str) -> Option<SegmentBase> {
    if url.is_empty() {
        return None;
    }
    let response = http
        .get(url)
        .header(reqwest::header::USER_AGENT, "Mozilla/5.0 BiliDroid/7.76.0")
        .header(reqwest::header::RANGE, "bytes=0-65535")
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    // Do not trust a CDN that ignores Range and responds with the complete
    // object: only the first 64 KiB are needed to find ftyp/moov/sidx.
    let mut bytes = Vec::with_capacity(64 * 1024);
    response.take(64 * 1024).read_to_end(&mut bytes).ok()?;
    parse_segment_base(&bytes)
}

fn parse_segment_base(bytes: &[u8]) -> Option<SegmentBase> {
    let mut offset = 0_usize;
    let mut initialization_end = None;
    let mut index_range = None;
    while offset.checked_add(8)? <= bytes.len() {
        let start = offset;
        let size32 = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as u64;
        let box_type = &bytes[offset + 4..offset + 8];
        let (header_size, size) = if size32 == 1 {
            if offset.checked_add(16)? > bytes.len() {
                return None;
            }
            (
                16_usize,
                u64::from_be_bytes(bytes[offset + 8..offset + 16].try_into().ok()?),
            )
        } else if size32 == 0 {
            (8_usize, (bytes.len() - offset) as u64)
        } else {
            (8_usize, size32)
        };
        if size < header_size as u64 {
            return None;
        }
        let end = offset.checked_add(usize::try_from(size).ok()?)?;
        if end > bytes.len() {
            // The requested prefix ended in a partial box. The ranges cannot
            // be trusted until the complete box is available.
            break;
        }
        match box_type {
            b"moov" => initialization_end = Some(end - 1),
            b"sidx" => index_range = Some((start, end - 1)),
            _ => {}
        }
        offset = end;
    }
    Some(SegmentBase {
        initialization: format!("0-{}", initialization_end?),
        index_range: {
            let (start, end) = index_range?;
            format!("{start}-{end}")
        },
    })
}

fn web_codec(codecid: u32, width: u32, height: u32, frame_rate: &str) -> &'static str {
    let fps = frame_rate
        .split_once('/')
        .map(|(num, den)| {
            let num = num.parse::<f32>().unwrap_or_default();
            let den = den.parse::<f32>().unwrap_or(1.0);
            num / den
        })
        .or_else(|| frame_rate.parse::<f32>().ok())
        .unwrap_or_default();
    match codecid {
        // Bilibili codecid 7 is AVC. The web player expects an RFC 6381
        // codec string; exposing `codecid-7` makes Firefox reject the DASH
        // representation before the requested quality can be rendered.
        // Bilibili's 1080p60 H.264 stream is High@L5.0 (`...0032`), while
        // 1080p30 uses High@L4.0 (`...0028`). Advertising the lower level for
        // a 60fps representation makes Firefox reject the representation.
        7 if (width >= 1920 || height >= 1080) && fps >= 50.0 => "avc1.640032",
        7 if width >= 1920 || height >= 1080 => "avc1.640028",
        7 if (width >= 1280 || height >= 720) && fps >= 50.0 => "avc1.64002a",
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
        // NOCODE=0, CODE264=1, CODE265=2, CODEAV1=3. The 8K (qn 127) stream is
        // HEVC-only and the server only emits it when the request prefers
        // CODE265; every other quality also exists as AVC, which Firefox can
        // always decode, so HEVC preference stays scoped to 8K requests. The
        // env override exists to experiment with codec preferences.
        let prefer_codec_type = std::env::var("BILI_PREFER_CODEC")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(if request.qn.unwrap_or(112) >= 127 {
                2
            } else {
                0
            });
        Self {
            vod: Some(VideoVod {
                aid: request.aid.unwrap_or(0),
                cid: request.cid,
                qn: request.qn.unwrap_or(112),
                fnver: 0,
                // The server only emits the 8K (qn 127) stream when the request
                // advertises capability bit 0x400, which the Android app ORs in
                // from IjkOptionsHelper::is_support_8k(). 4048 misses it.
                fnval: request.fnval.unwrap_or(DEFAULT_FNVAL) | 0x400,
                download: 0,
                force_host: 2,
                fourk: request.fourk.unwrap_or(true),
                prefer_codec_type,
                voice_balance: 0,
                // Trial eligibility is what makes the server hand out VIP
                // quality streams to this session; keep it enabled.
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
        let env = |key: &str| std::env::var(key).unwrap_or_default();
        Self {
            app_id: 1,
            build: DEFAULT_BUILD,
            buvid,
            mobi_app: "android".into(),
            platform: "android".into(),
            device: env("BILI_DEVICE_DEVICE"),
            channel: "master".into(),
            brand: if env("BILI_DEVICE_BRAND").is_empty() {
                "unknown".into()
            } else {
                env("BILI_DEVICE_BRAND")
            },
            model: if env("BILI_DEVICE_MODEL").is_empty() {
                "unknown".into()
            } else {
                env("BILI_DEVICE_MODEL")
            },
            osver: if env("BILI_DEVICE_OSVER").is_empty() {
                "unknown".into()
            } else {
                env("BILI_DEVICE_OSVER")
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn box_bytes(name: &[u8; 4], payload_len: usize) -> Vec<u8> {
        let size = u32::try_from(payload_len + 8).unwrap();
        let mut bytes = Vec::with_capacity(payload_len + 8);
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(name);
        bytes.resize(payload_len + 8, 0);
        bytes
    }

    #[test]
    fn parses_initialization_and_sidx_ranges() {
        let mut bytes = box_bytes(b"ftyp", 4);
        bytes.extend(box_bytes(b"moov", 12));
        bytes.extend(box_bytes(b"sidx", 20));
        let base = parse_segment_base(&bytes).unwrap();
        assert_eq!(base.initialization, "0-31");
        assert_eq!(base.index_range, "32-59");
    }

    #[test]
    fn rejects_a_truncated_mp4_box() {
        let mut bytes = box_bytes(b"ftyp", 4);
        bytes.extend_from_slice(&32_u32.to_be_bytes());
        bytes.extend_from_slice(b"moov");
        assert!(parse_segment_base(&bytes).is_none());
    }

    #[test]
    fn advertises_the_h264_level_for_1080p60() {
        assert_eq!(web_codec(7, 1920, 1080, "60.000"), "avc1.640032");
        assert_eq!(web_codec(7, 1920, 1080, "30.003"), "avc1.640028");
    }
}
