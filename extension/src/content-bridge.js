(() => {
  const source = document.createElement("script");
  source.src = browser.runtime.getURL("src/page-bridge.js");
  source.dataset.biliWebAndroidStream = "1";
  (document.documentElement || document.head).appendChild(source);
  source.remove();

  window.addEventListener("message", async (event) => {
    if (event.source !== window) return;
    const message = event.data;
    if (!message || message.source !== "biliwebandroidstream-page") return;
    if (message.type !== "bili-playurl-request") return;

    const response = await browser.runtime.sendMessage({
      type: "bili-playurl-request",
      url: message.url,
      method: message.method,
      bvid: message.bvid,
      aid: message.aid,
      cid: message.cid,
      qn: message.qn,
      fnval: message.fnval,
      fourk: message.fourk
    });
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
    const response = await browser.runtime.sendMessage({ type: "playback-auth" });
    window.postMessage({
      source: "biliwebandroidstream-content",
      type: "bili-playback-auth-response",
      id: event.data.id,
      response
    }, "*");
  });
})();
