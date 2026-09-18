// The former Rust native helper runs as plain JS in this background page
// (see native-api.js): passport QR login/refresh, the PlayViewUnite gRPC
// resolver, and token storage in browser.storage.local.

const requestOrigins = new Map();

// Two kinds of signed stream URLs flow through the bilivideo hosts and they
// have opposite header requirements:
//   - the page's boot-time `__playinfo__` URLs (platform=pc) only work with
//     completely stock browser headers; mutating them 403s and wedges the
//     player on "Timeout:20s" before the helper's response even arrives.
//   - the Android stream URLs (platform=android) 403 as soon as the request
//     carries a Referer, on most CDN families (upos-*, cn-*-cm, mountaintoys
//     PCDN) while the mcdn hosts tolerate it.
// So only touch the requests that are actually ours: strip Referer (and the
// Sec-Fetch metadata, which some edges dislike) from Android-platform URLs and
// leave every other request exactly as the page issued it. Origin stays: CDN
// mirrors echo it into Access-Control-Allow-Origin, which the cross-origin
// segment XHRs need.
try {
  browser.webRequest.onBeforeSendHeaders.addListener(
    (details) => {
      if (!/[?&]platform=android(_tv_yst)?(?=&|$)/.test(details.url)) return {};
      return {
        requestHeaders: (details.requestHeaders || [])
          .filter((header) => !["referer", "sec-fetch-site", "sec-fetch-mode", "sec-fetch-dest"].includes(header.name.toLowerCase()))
      };
    },
    { urls: ["*://*.bilivideo.com/*", "*://*.bilivideo.cn/*", "*://*.mountaintoys.cn/*"] },
    ["blocking", "requestHeaders", "extraHeaders"]
  );
} catch (_) {
  // Older Firefox builds may reject the extraHeaders option. Retry without it.
  try {
    browser.webRequest.onBeforeSendHeaders.addListener(
      (details) => {
        if (!/[?&]platform=android(_tv_yst)?(?=&|$)/.test(details.url)) return {};
        return {
          requestHeaders: (details.requestHeaders || [])
            .filter((header) => !["referer", "sec-fetch-site", "sec-fetch-mode", "sec-fetch-dest"].includes(header.name.toLowerCase()))
        };
      },
      { urls: ["*://*.bilivideo.com/*", "*://*.bilivideo.cn/*", "*://*.mountaintoys.cn/*"] },
      ["blocking", "requestHeaders"]
    );
  } catch (_) {
    // Native playback remains usable when request-header filtering is absent.
  }
}

// The extension's background fetches carry `Origin: moz-extension://...`,
// which bilibili's WAF answers with an HTML block page instead of JSON (the
// page context works because it sends the site origin). Strip the Origin for
// passport/api requests that originate from this extension; the endpoints are
// origin-agnostic (they authenticate via signed params / access_key).
try {
  browser.webRequest.onBeforeSendHeaders.addListener(
    (details) => {
      const originUrl = details.originUrl || details.documentUrl || "";
      if (!originUrl.startsWith("moz-extension://")) return {};
      return {
        requestHeaders: (details.requestHeaders || []).filter(
          (header) => header.name.toLowerCase() !== "origin"
        )
      };
    },
    { urls: ["https://passport.bilibili.com/*", "https://api.bilibili.com/*", "https://grpc.biliapi.net/*"] },
    ["blocking", "requestHeaders"]
  );
} catch (_) {
  // Without header filtering the WAF may block background requests; the
  // options page calls would fail while page-context playback still works.
}

browser.runtime.onMessage.addListener((message, sender) => {
  if (!message || typeof message.type !== "string") return undefined;
  console.log('[BiliWAS] bg message', message.type, 'qn', message.qn ?? '');

  if (message.type === "bili-playurl-request") {
    return BiliNative.resolvePlayUrl({
      bvid: message.bvid || "",
      aid: message.aid == null ? null : Number(message.aid),
      cid: Number(message.cid || 0),
      qn: message.qn == null ? null : Number(message.qn),
      fnval: message.fnval == null ? null : Number(message.fnval),
      fourk: message.fourk == null ? null : Boolean(message.fourk),
      page_url: sender && sender.tab ? sender.tab.url || "" : ""
    }).then((response) => {
      console.log('[BiliWAS] playurl answer type', response.type, 'code', response.code || '', 'quality', response.response?.quality ?? '');
      // Hand the raw Android response to the page bridge: the player's
      // DashBilibiliParser only accepts manifests that carry MP4
      // segment_base ranges, and the page builds the final body by splicing
      // the Android streams into the official web response (which already
      // has every field the parser expects).
      if (response.type === "play_url" && response.response) {
        return { ok: true, android: response.response, requestedQuality: message.qn };
      }
      return response;
    }).catch((error) => ({
      ok: false,
      code: "resolve_failed",
      message: String(error)
    }));
  }

  if (message.type === "token-import") {
    return BiliNative.importTokenJson(message.token_json)
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "invalid_token", message: String(error) }));
  }

  if (message.type === "token-clear") {
    return BiliNative.clearToken()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "storage_error", message: String(error) }));
  }

  if (message.type === "helper-info") {
    return Promise.resolve(BiliNative.info());
  }

  if (message.type === "helper-status") {
    return BiliNative.tokenStatus()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "storage_error", message: String(error) }));
  }

  if (message.type === "helper-refresh") {
    return BiliNative.refresh()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "token_refresh_failed", message: String(error) }));
  }

  if (message.type === "playback-auth") {
    return BiliNative.playbackAuth()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "playback_auth_failed", message: String(error), stack: String(error.stack || "").split("\n").slice(0, 5).join(" | ") }));
  }

  if (message.type === "qr-start") {
    return BiliNative.qrStart()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "qr_start_failed", message: String(error) }));
  }

  if (message.type === "qr-poll") {
    return BiliNative.qrPoll()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "qr_poll_failed", message: String(error) }));
  }

  if (message.type === "set-buvid") {
    return BiliNative.setBuvid(message.buvid)
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "set_buvid_failed", message: String(error) }));
  }

  if (message.type === "web-session-status") {
    return BiliNative.webSessionStatus()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "web_session_status_failed", message: String(error) }));
  }

  if (message.type === "web-cookie-login") {
    return BiliNative.webCookieLogin()
      .then((response) => response)
      .catch((error) => ({ type: "error", code: "web_cookie_login_failed", message: String(error) }));
  }

  return undefined;
});
