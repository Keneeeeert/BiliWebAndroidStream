(() => {
  if (window.__BILI_WEB_ANDROID_STREAM__) return;
  window.__BILI_WEB_ANDROID_STREAM__ = true;

  const nativeFetch = window.fetch.bind(window);
  let nextId = 1;
  const pending = new Map();

  function rewriteAccountBody(body) {
    if (!body || body.code !== 0 || !body.data) return body;
    const data = body.data;
    data.vip = { ...(data.vip || {}), type: 2, status: 1 };
    data.options = { ...(data.options || {}), without_vip: false };
    return body;
  }

  function rewriteXhrResponse(xhr) {
    if (xhr.__biliWebAndroidStreamRewritten) return;
    xhr.__biliWebAndroidStreamRewritten = true;
    try {
      const text = xhr.responseType && xhr.responseType !== "text" && xhr.responseType !== ""
        ? new TextDecoder().decode(xhr.response)
        : xhr.responseText;
      const body = rewriteAccountBody(JSON.parse(text));
      const rewritten = JSON.stringify(body);
      Object.defineProperty(xhr, "responseText", { configurable: true, value: rewritten });
      Object.defineProperty(xhr, "response", { configurable: true, value: rewritten });
    } catch (_) {
      // Preserve the original response if the endpoint changes shape.
    }
  }

  function replaceXhrJson(xhr, body) {
    const text = JSON.stringify(body);
    try { Object.defineProperty(xhr, "responseText", { configurable: true, value: text }); } catch (_) {}
    try { Object.defineProperty(xhr, "response", { configurable: true, value: text }); } catch (_) {}
  }

  const nativeXhrOpen = XMLHttpRequest.prototype.open;
  const nativeXhrSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url, ...rest) {
    const normalizedUrl = String(url);
    this.__biliWebAndroidStreamAccountRequest = normalizedUrl.includes("/x/player/wbi/v2");
    this.__biliWebAndroidStreamPlayRequest = normalizedUrl.includes("/x/player/wbi/playurl");
    this.__biliWebAndroidStreamRequestUrl = normalizedUrl;
    this.__biliWebAndroidStreamRequestMethod = method;
    return nativeXhrOpen.call(this, method, url, ...rest);
  };
  XMLHttpRequest.prototype.send = function(body) {
    if (this.__biliWebAndroidStreamPlayRequest) {
      const xhr = this;
      const onLoadEnd = xhr.onloadend;
      let nativeDone = false;
      let helperDone = false;
      let helperAnswer;
      let nativeEvent;
      const finish = () => {
        if (!nativeDone || !helperDone) return;
        if (helperAnswer?.ok && helperAnswer.body) replaceXhrJson(xhr, helperAnswer.body);
        if (typeof onLoadEnd === "function") onLoadEnd.call(xhr, nativeEvent);
      };
      xhr.onloadend = (event) => {
        nativeDone = true;
        nativeEvent = event;
        finish();
      };
      askHelper(xhr.__biliWebAndroidStreamRequestUrl, xhr.__biliWebAndroidStreamRequestMethod, body)
        .then((answer) => { helperAnswer = answer; helperDone = true; finish(); })
        .catch(() => { helperDone = true; finish(); });
      return nativeXhrSend.call(xhr, body);
    }
    if (!this.__biliWebAndroidStreamAccountRequest) {
      return nativeXhrSend.call(this, body);
    }
    const onLoadEnd = this.onloadend;
    const onReadyStateChange = this.onreadystatechange;
    this.onloadend = (event) => {
      rewriteXhrResponse(this);
      if (typeof onLoadEnd === "function") onLoadEnd.call(this, event);
    };
    this.onreadystatechange = (event) => {
      if (this.readyState === 4) rewriteXhrResponse(this);
      if (typeof onReadyStateChange === "function") onReadyStateChange.call(this, event);
    };
    return nativeXhrSend.call(this, body);
  };

  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const message = event.data;
    if (!message || message.source !== "biliwebandroidstream-content") return;
    if (message.type !== "bili-playurl-response") return;
    const resolver = pending.get(message.id);
    if (!resolver) return;
    pending.delete(message.id);
    resolver(message.response);
  });


  function askHelper(url, method, body) {
    const id = nextId++;
    const parsed = new URL(url, window.location.href);
    const params = parsed.searchParams;
    if (typeof body === "string" && body) {
      try {
        for (const [key, value] of new URLSearchParams(body)) {
          if (!params.has(key)) params.set(key, value);
        }
      } catch (_) {}
    }
    return new Promise((resolve) => {
      pending.set(id, resolve);
      window.postMessage({
        source: "biliwebandroidstream-page",
        type: "bili-playurl-request",
        id,
        url,
        method,
        bvid: params.get("bvid") || "",
        aid: params.get("avid") ? Number(params.get("avid")) : (params.get("aid") ? Number(params.get("aid")) : (window.__INITIAL_STATE__?.aid || null)),
        cid: params.get("cid") ? Number(params.get("cid")) : 0,
        qn: params.get("qn") ? Number(params.get("qn")) : null,
        fnval: params.get("fnval") ? Number(params.get("fnval")) : null,
        fourk: params.get("fourk") === "1"
      }, "*");
      window.setTimeout(() => {
        if (!pending.has(id)) return;
        pending.delete(id);
        resolve({ ok: false, code: "bridge_timeout" });
      }, 2500);
    });
  }

  window.fetch = async function(input, init) {
    const request = new Request(input, init);
    const url = request.url;
    if (url.includes("/x/player/wbi/v2")) {
      const response = await nativeFetch(input, init);
      try {
        const body = rewriteAccountBody(await response.clone().json());
        return new Response(JSON.stringify(body), {
          status: response.status,
          statusText: response.statusText,
          headers: response.headers
        });
      } catch (_) {
        return response;
      }
    }
    if (!url.includes("/x/player/wbi/playurl")) {
      return nativeFetch(input, init);
    }

    // The helper is only allowed to replace a response when it explicitly
    // returns a normalized web-compatible JSON body. Otherwise preserve the
    // official web response unchanged.
    const requestBody = request.method === "GET" || request.method === "HEAD"
      ? null
      : await request.clone().text();
    const answer = await askHelper(url, request.method, requestBody);
    if (answer?.ok && answer.body) {
      return new Response(JSON.stringify(answer.body), {
        status: 200,
        headers: { "content-type": "application/json; charset=utf-8" }
      });
    }
    return nativeFetch(input, init);
  };

  const accountReloadTimer = window.setInterval(() => {
    if (!window.player || typeof window.player.reloadAccess !== "function") return;
    window.clearInterval(accountReloadTimer);
    try { window.player.reloadAccess(); } catch (_) { /* page may not be ready */ }
    if (typeof window.player.setRequestedQuality === "function") {
      // The stock web player rejects qn >= 100 through its VIP guide before
      // issuing the playurl request. The Android response already contains
      // the authorized stream, so route the public method directly to the
      // quality state machine and let the normal media pipeline handle it.
      window.player.requestQuality = function(quality, audio) {
        return this.setRequestedQuality(quality, audio == null ? null : Number(audio));
      };
    }
  }, 500);
})();
