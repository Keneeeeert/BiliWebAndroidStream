# Native helper protocol (draft)

The Rust binary is a Firefox Native Messaging host. It reads and writes one
JSON message per frame on standard input/output. Each frame is a four-byte
little-endian payload length followed by UTF-8 JSON. Payloads are capped at
8 MiB.

The helper keeps the imported Android token in its per-user configuration
directory with restrictive file permissions (`0600` on Unix). It does not
inspect Firefox profile databases or log token contents. The extension reads
the user-selected JSON file and sends its text in `set_token`; the helper
normalizes and stores only the supported token fields.

## Requests

```json
{"request_id":"1","type":"ping"}
{"request_id":"2","type":"status"}
{"request_id":"3","type":"set_token","token_json":"{\"access_key\":\"...\"}"}
{"request_id":"4","type":"clear_token"}
{"request_id":"5","type":"web_cookie_login","sessdata":"...","dede_user_id":"...","bili_jct":"..."}
{"request_id":"6","type":"qr_start"}
{"request_id":"7","type":"qr_poll"}
{"request_id":"8","type":"refresh_token"}
{"request_id":"9","type":"resolve_play_url","bvid":"BV...","aid":1234,"cid":123,"qn":112,"fnval":4048,"fourk":true}
```

`set_token` accepts a flat Android token object or a `data`, `token`,
`token_info`, or `result` wrapper. The required field is `access_key` (with
`access_token` accepted as a compatibility alias); `refresh_token`, `mid`,
`expires`/`expires_in`, `fast_login_token`, `buvid`, and `app_key` are
optional. Responses never include secret fields.

## Responses

Successful responses use the matching `request_id`:

```json
{"request_id":"1","type":"pong"}
{"request_id":"2","type":"status","configured":false,"has_refresh_token":false}
{"request_id":"3","type":"token_set","status":{"configured":true,"has_refresh_token":true,"expires_at":1730000000}}
{"request_id":"4","type":"token_cleared"}
```

Errors have this shape:

```json
{"request_id":"5","type":"error","code":"missing_token","message":"a token is required before requesting playback"}
```

`resolve_play_url` calls the Android `PlayViewUnite` gRPC method. If the
server does not return a DASH URL for the requested quality, the helper
returns `playback_response_error`; it does not invent a URL. The normalized
response contains video and audio DASH URLs so the extension can rebuild the
web player JSON shape.

`web_cookie_login` is experimental. The extension obtains only the three
required Bilibili cookies through Firefox's cookies API and sends them to the
local helper. The helper performs the official Passport cookie-confirm/poll
sequence and stores the returned token locally; cookies are not sent to any
third-party service.

`qr_start` returns the official QR URL and a locally rendered `qr_svg`; the
extension displays that SVG directly so the QR URL is not mistaken for a page
that must be opened inside the Bilibili client. `qr_poll` completes the APP
token flow after the user confirms on the phone.

`refresh_token` calls the host-compatible Passport refresh endpoint. Playback
also refreshes automatically when the stored token is within one minute of
expiry and has a refresh token. A rotated refresh token is persisted together
with the new access token.
