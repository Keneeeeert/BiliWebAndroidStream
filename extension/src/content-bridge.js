(() => {
  // Firefox's chrome.runtime.sendMessage does not carry promise responses;
  // route messaging through browser when available (Chrome falls back).
  const messaging = globalThis.browser || globalThis.chrome;
  const source = document.createElement("script");
  source.src = chrome.runtime.getURL("src/page-bridge.js");
  source.dataset.biliWebAndroidStream = "1";
  (document.documentElement || document.head).appendChild(source);
  source.remove();

  window.addEventListener("message", async (event) => {
    if (event.source !== window) return;
    const message = event.data;
    if (!message || message.source !== "biliwebandroidstream-page") return;
    if (message.type !== "bili-playurl-request") return;

    const response = await messaging.runtime.sendMessage({
      type: "bili-playurl-request",
      url: message.url,
      method: message.method,
      bvid: message.bvid,
      aid: message.aid,
      cid: message.cid,
      qn: message.qn,
      fnval: message.fnval,
      fourk: message.fourk
    }).catch((error) => ({ ok: false, code: "content_bridge_error", message: String(error) }));
    window.postMessage({
      source: "biliwebandroidstream-content",
      type: "bili-playurl-response",
      id: message.id,
      response
    }, "*");
  });

  window.addEventListener("message", async (event) => {
    if (event.source !== window || event.data?.source !== "biliwebandroidstream-page") return;
    if (event.data.type !== "bili-playback-auth-request") return;
    const response = await messaging.runtime.sendMessage({ type: "playback-auth" })
      .catch((error) => ({ ok: false, code: "content_bridge_error", message: String(error) }));
    window.postMessage({
      source: "biliwebandroidstream-content",
      type: "bili-playback-auth-response",
      id: event.data.id,
      response
    }, "*");
  });
})();
