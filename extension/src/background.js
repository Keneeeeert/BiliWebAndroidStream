const NATIVE_HOST = "com.biliwebandroidstream.helper";
let playbackAuthorization = null;
let nativePort = null;
let nativeSequence = 1;
const nativePending = new Map();

function sendNative(message) {
  const request = browser.runtime.sendNativeMessage(NATIVE_HOST, message);
  const timer = new Promise((_, reject) => setTimeout(() => reject(new Error("native helper timeout")), 15_000));
  return Promise.race([request, timer]);
}

function sendNativePersistent(message) {
  if (!nativePort) {
    nativePort = browser.runtime.connectNative(NATIVE_HOST);
    nativePort.onMessage.addListener((response) => {
      const requestId = response?.request_id;
      const pending = requestId && nativePending.get(requestId);
      if (!pending) return;
      nativePending.delete(requestId);
      clearTimeout(pending.timer);
      pending.resolve(response);
    });
    nativePort.onDisconnect.addListener(() => {
      const error = new Error(browser.runtime.lastError?.message || "native host disconnected");
      for (const pending of nativePending.values()) {
        clearTimeout(pending.timer);
        pending.reject(error);
      }
      nativePending.clear();
      nativePort = null;
    });
  }
  const requestId = `persistent-${nativeSequence++}`;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      const pending = nativePending.get(requestId);
      if (!pending) return;
      nativePending.delete(requestId);
      pending.reject(new Error("native helper timeout"));
    }, 15_000);
    nativePending.set(requestId, { resolve, reject, timer });
    try {
      nativePort.postMessage({ ...message, request_id: requestId });
    } catch (error) {
      nativePending.delete(requestId);
      clearTimeout(timer);
      reject(error);
    }
  });
}

function cachePlaybackAuth(response) {
  if (response?.type === "playback_auth" && typeof response.authorization === "string") {
    playbackAuthorization = response.authorization;
  }
  return response;
}

sendNative({ type: "playback_auth" }).then(cachePlaybackAuth).catch(() => {});

// Two kinds of signed stream URLs flow through the bilivideo hosts and they
// have opposite header requirements:
//   - the page's boot-time `__playinfo__` URLs (platform=pc) only work with
//     completely stock browser headers; mutating them 403s and wedges the
//     player on "Timeout:20s" before the helper's response even arrives.
//   - the helper's Android stream URLs (platform=android) 403 as soon as the
//     request carries a Referer, on most CDN families (upos-*, cn-*-cm,
//     mountaintoys PCDN) while the mcdn hosts tolerate it.
// So only touch the requests that are actually ours: strip Referer (and the
// Sec-Fetch metadata, which some edges dislike) from Android-platform URLs and
// leave every other request exactly as the page issued it. Origin stays: CDN
// mirrors echo it into Access-Control-Allow-Origin, which the cross-origin
// segment XHRs need.
try {
  browser.webRequest.onBeforeSendHeaders.addListener(
    (details) => {
      if (!/[?&]platform=android(?=&|$)/.test(details.url)) return {};
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
        if (!/[?&]platform=android(?=&|$)/.test(details.url)) return {};
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


browser.runtime.onMessage.addListener((message, sender) => {
  if (!message || typeof message.type !== "string") return undefined;

  if (message.type === "bili-playurl-request") {
    return sendNative({
      type: "resolve_play_url",
      bvid: message.bvid || "",
      aid: message.aid == null ? null : Number(message.aid),
      cid: Number(message.cid || 0),
      qn: message.qn == null ? null : Number(message.qn),
      fnval: message.fnval == null ? null : Number(message.fnval),
      fourk: message.fourk == null ? null : Boolean(message.fourk),
      page_url: sender?.tab?.url || ""
    }).then((response) => {
      if (response?.type === "play_url" && response.response) {
        // Hand the raw Android response to the page bridge: the player's
        // DashBilibiliParser only accepts manifests that carry MP4
        // segment_base ranges, and the page builds the final body by splicing
        // the Android streams into the official web response (which already
        // has every field the parser expects).
        return { ok: true, android: response.response, requestedQuality: message.qn };
      }
      return response || { ok: false, code: "empty_native_response" };
    }).catch((error) => ({
      ok: false,
      code: "native_helper_unavailable",
      message: String(error)
    }));
  }

  if (message.type === "token-import") {
    return sendNative({ type: "set_token", token_json: message.token_json })
      .then((response) => response || { ok: true })
      .catch((error) => ({ ok: false, code: "native_helper_unavailable", message: String(error) }));
  }

  if (message.type === "token-clear") {
    return sendNative({ type: "clear_token" })
      .then((response) => response || { ok: true })
      .catch((error) => ({ ok: false, code: "native_helper_unavailable", message: String(error) }));
  }

  if (message.type === "helper-info") {
    return sendNative({ type: "info" })
      .then((response) => response || { ok: false, code: "empty_native_response" })
      .catch((error) => ({ ok: false, code: "native_helper_unavailable", message: String(error) }));
  }

  if (message.type === "helper-status") {
    return sendNative({ type: "status" })
      .then((response) => response || { ok: true })
      .catch((error) => ({ ok: false, code: "native_helper_unavailable", message: String(error) }));
  }

  if (message.type === "helper-refresh") {
    return sendNative({ type: "refresh_token" })
      .then((response) => response || { ok: true })
      .catch((error) => ({ ok: false, code: "token_refresh_failed", message: String(error) }));
  }

  if (message.type === "playback-auth") {
    return sendNative({ type: "playback_auth" })
      .then((response) => {
        cachePlaybackAuth(response);
        return response || { ok: false, code: "empty_native_response" };
      })
      .catch((error) => ({ ok: false, code: "playback_auth_failed", message: String(error) }));
  }

  if (message.type === "qr-start") {
    return sendNativePersistent({ type: "qr_start" })
      .then((response) => response || { ok: false, code: "empty_native_response" })
      .catch((error) => ({ ok: false, code: "qr_start_failed", message: String(error) }));
  }

  if (message.type === "qr-poll") {
    return sendNativePersistent({ type: "qr_poll" })
      .then((response) => response || { ok: false, code: "empty_native_response" })
      .catch((error) => ({ ok: false, code: "qr_poll_failed", message: String(error) }));
  }

  if (message.type === "set-buvid") {
    return sendNative({ type: "set_buvid", buvid: message.buvid })
      .then((response) => response || { ok: false, code: "empty_native_response" })
      .catch((error) => ({ ok: false, code: "set_buvid_failed", message: String(error) }));
  }

  if (message.type === "web-session-status") {
    return Promise.all([
      browser.cookies.get({ url: "https://www.bilibili.com/", name: "SESSDATA" }),
      browser.cookies.get({ url: "https://www.bilibili.com/", name: "DedeUserID" }),
      browser.cookies.get({ url: "https://www.bilibili.com/", name: "bili_jct" })
    ]).then(([sessdata, dedeUserId, biliJct]) => ({
      type: "web_session_status",
      logged_in: Boolean(sessdata?.value && dedeUserId?.value && biliJct?.value)
    })).catch((error) => ({ ok: false, code: "web_session_status_failed", message: String(error) }));
  }

  if (message.type === "web-cookie-login") {
    return Promise.all([
      browser.cookies.get({ url: "https://www.bilibili.com/", name: "SESSDATA" }),
      browser.cookies.get({ url: "https://www.bilibili.com/", name: "DedeUserID" }),
      browser.cookies.get({ url: "https://www.bilibili.com/", name: "bili_jct" })
    ]).then(([sessdata, dedeUserId, biliJct]) => {
      if (!sessdata?.value || !dedeUserId?.value || !biliJct?.value) {
        return { ok: false, code: "web_session_missing", message: "当前 bilibili.com 登录态缺少必要 Cookie" };
      }
      return sendNative({
        type: "web_cookie_login",
        sessdata: sessdata.value,
        dede_user_id: dedeUserId.value,
        bili_jct: biliJct.value
      });
    }).catch((error) => ({ ok: false, code: "web_cookie_login_failed", message: String(error) }));
  }

  return undefined;
});
