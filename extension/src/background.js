const NATIVE_HOST = "com.biliwebandroidstream.helper";
let playbackAuthorization = null;
let nativePort = null;
let nativeSequence = 1;
const nativePending = new Map();

function sendNative(message) {
  return new Promise((resolve, reject) => {
    browser.runtime.sendNativeMessage(NATIVE_HOST, message).then(resolve, reject);
  });
}

function sendNativePersistent(message) {
  if (!nativePort) {
    nativePort = browser.runtime.connectNative(NATIVE_HOST);
    nativePort.onMessage.addListener((response) => {
      const requestId = response?.request_id;
      const pending = requestId && nativePending.get(requestId);
      if (!pending) return;
      nativePending.delete(requestId);
      pending.resolve(response);
    });
    nativePort.onDisconnect.addListener(() => {
      const error = new Error(browser.runtime.lastError?.message || "native host disconnected");
      for (const pending of nativePending.values()) pending.reject(error);
      nativePending.clear();
      nativePort = null;
    });
  }
  const requestId = `persistent-${nativeSequence++}`;
  return new Promise((resolve, reject) => {
    nativePending.set(requestId, { resolve, reject });
    try {
      nativePort.postMessage({ ...message, request_id: requestId });
    } catch (error) {
      nativePending.delete(requestId);
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

browser.webRequest.onBeforeSendHeaders.addListener(
  (details) => {
    if (!playbackAuthorization) return {};
    const headers = details.requestHeaders || [];
    const existing = headers.find((header) => header.name.toLowerCase() === "authorization");
    if (existing) existing.value = playbackAuthorization;
    else headers.push({ name: "Authorization", value: playbackAuthorization });
    return { requestHeaders: headers };
  },
  { urls: ["*://*.bilivideo.com/*", "*://*.bilivideo.cn/*", "*://*.mountaintoys.cn/*"] },
  ["blocking", "requestHeaders", "extraHeaders"]
);

function toWebPlayInfo(response, requestedQuality) {
  const videos = (response.streams || []).map((stream) => ({
    id: stream.quality,
    baseUrl: stream.base_url,
    base_url: stream.base_url,
    backupUrl: stream.backup_urls || [],
    backup_url: stream.backup_urls || [],
    mimeType: stream.mime_type,
    mime_type: stream.mime_type,
    codecs: stream.codecs || "",
    width: stream.width,
    height: stream.height,
    bandwidth: stream.bandwidth
  }));
  const audio = (response.audio || []).map((item) => ({
    id: item.id,
    baseUrl: item.base_url,
    base_url: item.base_url,
    backupUrl: item.backup_urls || [],
    backup_url: item.backup_urls || [],
    mimeType: item.mime_type,
    mime_type: item.mime_type,
    bandwidth: item.bandwidth
  }));
  const qualities = videos.map((item) => item.id);
  if (requestedQuality && !qualities.some((quality) => quality >= requestedQuality)) {
    return null;
  }
  return {
    code: 0,
    message: "OK",
    ttl: 1,
    data: {
      from: "local_android_grpc",
      result: "suee",
      message: "",
      quality: response.quality,
      format: "dash",
      timelength: response.duration_ms,
      accept_format: "dash",
      accept_description: qualities.map((quality) => `${quality}P`),
      accept_quality: qualities,
      dash: {
        duration: response.duration_ms / 1000,
        minBufferTime: 1.5,
        video: videos,
        audio
      },
      support_formats: qualities.map((quality) => ({
        quality,
        format: "dash",
        display_desc: `${quality}P`,
        need_login: false,
        need_vip: false,
        vip_free: true
      })),
      view_info: {}
    }
  };
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
      // Prime the webRequest header cache before dash.js starts requesting
      // the returned CDN segments.
      sendNative({ type: "playback_auth" }).then(cachePlaybackAuth).catch(() => {});
      if (response?.type === "play_url" && response.response) {
        const body = toWebPlayInfo(response.response, message.qn);
        return body ? { ok: true, body } : { ok: false, code: "requested_quality_unavailable" };
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
