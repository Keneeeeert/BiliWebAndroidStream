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

  // The page bridge reports a quality switch the official response cannot
  // deliver and the Android helper cannot fill in (no token yet): the player is
  // about to fail the switch with its own terse error. Say what is missing
  // first, in the page's top-right corner, so the reason arrives before the
  // failure does.
  const HINT_ID = "biliwas-login-hint";
  const HINT_STYLE_ID = "biliwas-login-hint-style";
  const HINT_TIMEOUT_MS = 10000;
  const HINT_ICON = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1"></rect><rect x="14" y="3" width="7" height="7" rx="1"></rect><rect x="3" y="14" width="7" height="7" rx="1"></rect><path d="M14 14h3v3h-3z"></path><path d="M20.5 14v.01M14 20.5v.01M20.5 20.5v.01M17.5 17.5v.01"></path></svg>';
  // The page's own stylesheets must not leak into the hint, hence the explicit
  // typography and the !important layout properties on the root element.
  const HINT_CSS = `
#biliwas-login-hint {
  position: fixed !important; top: 16px !important; right: 16px !important;
  z-index: 2147483647 !important; display: flex !important; align-items: center !important;
  gap: 10px !important; max-width: 420px !important; padding: 10px 12px !important;
  border: 1px solid rgba(251, 114, 153, .55) !important; border-radius: 10px !important;
  background: rgba(22, 23, 26, .94) !important; box-shadow: 0 8px 28px rgba(0, 0, 0, .4) !important;
  color: #fff !important; text-align: left !important;
  font: 13px/1.5 "Microsoft YaHei", "PingFang SC", system-ui, sans-serif !important;
}
/* The entrance only slides, never fades: Chrome freezes animations at their
   first keyframe in occluded windows, and a hint that never fades in is a hint
   nobody sees. */
@media (prefers-reduced-motion: no-preference) {
  #biliwas-login-hint { animation: biliwas-login-hint-in .22s ease-out !important; }
  @keyframes biliwas-login-hint-in {
    from { transform: translateY(-8px); }
    to { transform: none; }
  }
}
#biliwas-login-hint .biliwas-hint-icon { flex: 0 0 auto; display: flex; color: #fb7299; }
#biliwas-login-hint .biliwas-hint-icon svg { width: 18px; height: 18px; }
#biliwas-login-hint .biliwas-hint-text { flex: 1 1 auto; }
#biliwas-login-hint button { flex: 0 0 auto; border: 0; cursor: pointer; font-family: inherit; }
#biliwas-login-hint .biliwas-hint-action { padding: 4px 10px; border-radius: 6px; background: #fb7299; color: #fff; font-size: 12px; line-height: 1.4; }
#biliwas-login-hint .biliwas-hint-action:hover { background: #ff8fb0; }
#biliwas-login-hint .biliwas-hint-close { padding: 0 2px; background: none; color: #9aa0a6; font-size: 16px; line-height: 1; }
#biliwas-login-hint .biliwas-hint-close:hover { color: #fff; }
`;
  let hintTimer = null;
  let toastWatchTimer = null;
  let toastObserver = null;

  // The player answers a failed manual switch with its own "切换失败" toast a few
  // seconds after the fallback. That message carries no reason, and the hint
  // above already gives one, so drop the toast while a hint is up — and only
  // then: failures the hint cannot explain (a network hiccup, a dead CDN) must
  // keep the player's own report instead of going silent.
  const TOAST_WATCH_MS = 20000;
  const TOAST_FAILURE = /切换失败/;

  function dropFailureToasts() {
    for (const row of document.querySelectorAll('.bpx-player-toast-row, [class*="bpx-player-toast"]')) {
      const text = (row.textContent || '').trim();
      if (text && TOAST_FAILURE.test(text)) row.remove();
    }
  }

  function stopWatchingFailureToasts() {
    if (toastObserver) {
      toastObserver.disconnect();
      toastObserver = null;
    }
    if (toastWatchTimer !== null) {
      window.clearTimeout(toastWatchTimer);
      toastWatchTimer = null;
    }
  }

  function watchFailureToasts() {
    stopWatchingFailureToasts();
    dropFailureToasts();
    const root = document.body || document.documentElement;
    if (!root) return;
    toastObserver = new MutationObserver(dropFailureToasts);
    toastObserver.observe(root, { childList: true, subtree: true, characterData: true });
    toastWatchTimer = window.setTimeout(stopWatchingFailureToasts, TOAST_WATCH_MS);
  }

  // The player fills the whole viewport in page fullscreen, so the hint has to
  // hang inside the fullscreen element to stay visible there.
  function hintHost() {
    return document.fullscreenElement || document.body || document.documentElement;
  }

  function dismissLoginHint() {
    const hint = document.getElementById(HINT_ID);
    if (hint) hint.remove();
    if (hintTimer !== null) {
      window.clearTimeout(hintTimer);
      hintTimer = null;
    }
  }

  function showLoginHint(message) {
    dismissLoginHint();
    const host = hintHost();
    if (!host) return;
    const label = message.label;
    if (!document.getElementById(HINT_STYLE_ID)) {
      const style = document.createElement("style");
      style.id = HINT_STYLE_ID;
      style.textContent = HINT_CSS;
      host.appendChild(style);
    }
    const hint = document.createElement("div");
    hint.id = HINT_ID;
    hint.setAttribute("role", "alert");
    const icon = document.createElement("span");
    icon.className = "biliwas-hint-icon";
    icon.innerHTML = HINT_ICON;
    const text = document.createElement("span");
    text.className = "biliwas-hint-text";
    if (!label) {
      text.textContent = "需要手机扫码登录才能切换高清画质";
    } else if (message.webLogin) {
      text.textContent = `需要手机扫码登录才能切换到「${label}」`;
    } else {
      text.textContent = `请先登录才能切换到「${label}」`;
    }
    const action = document.createElement("button");
    action.type = "button";
    action.className = "biliwas-hint-action";
    action.textContent = "去登录";
    action.addEventListener("click", () => {
      messaging.runtime.sendMessage({ type: "open-options" }).catch(() => {});
      dismissLoginHint();
    });
    const close = document.createElement("button");
    close.type = "button";
    close.className = "biliwas-hint-close";
    close.title = "关闭";
    close.textContent = "×";
    close.addEventListener("click", dismissLoginHint);
    hint.append(icon, text, action, close);
    host.appendChild(hint);
    hintTimer = window.setTimeout(dismissLoginHint, HINT_TIMEOUT_MS);
    watchFailureToasts();
  }

  document.addEventListener("fullscreenchange", () => {
    const hint = document.getElementById(HINT_ID);
    if (hint) hintHost().appendChild(hint);
  });

  window.addEventListener("message", (event) => {
    if (event.source !== window) return;
    const message = event.data;
    if (!message || message.source !== "biliwebandroidstream-page") return;
    if (message.type !== "bili-login-hint") return;
    showLoginHint(message);
  });
})();
