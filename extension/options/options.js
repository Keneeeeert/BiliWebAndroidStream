const accountStatus = document.getElementById("account-status");
const accountBadge = document.getElementById("account-badge");
const webStatus = document.getElementById("web-status");
const helperStatus = document.getElementById("helper-status");
const helperBadge = document.getElementById("helper-badge");
const status = document.getElementById("status");
const qrBox = document.getElementById("qr-box");
const qrImage = document.getElementById("qr-image");
const RELEASES_URL = "https://github.com/Ujhhgtg/BiliWebAndroidStream/releases/latest";
const INSTALL_DOC_URL = "https://github.com/Ujhhgtg/BiliWebAndroidStream/blob/main/docs/helper-install.md";
const NATIVE_TIMEOUT_MS = 12_000;
let qrPolling = false;
let statusRun = 0;

function show(value, kind = "") {
  status.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  status.className = kind;
}

function sleep(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }

function timeout(ms, label) {
  return new Promise((_, reject) => setTimeout(() => reject(new Error(`${label}（${ms / 1000} 秒超时）`)), ms));
}

async function native(type, payload = {}) {
  let response;
  try {
    response = await Promise.race([
      browser.runtime.sendMessage({ type, ...payload }),
      timeout(NATIVE_TIMEOUT_MS, "helper 没有响应")
    ]);
  } catch (error) {
    throw new Error(error?.message || String(error));
  }
  if (response?.type === "error") throw new Error(response.message || response.code || "helper 请求失败");
  if (response?.ok === false || response?.code && response?.type !== "status") {
    throw new Error(response.message || response.code || "helper 请求失败");
  }
  return response;
}

async function runButton(button, pendingText, task) {
  if (button.disabled) return;
  const oldText = button.textContent;
  button.disabled = true;
  button.textContent = pendingText;
  try { await task(); }
  catch (error) { show(error?.message || String(error), "error"); }
  finally { button.disabled = false; button.textContent = oldText; }
}

function setBadge(element, text, kind = "") {
  element.textContent = text;
  element.className = `pill ${kind}`;
}

function formatExpiry(expiresAt) {
  return expiresAt ? new Date(expiresAt * 1000).toLocaleString() : "未提供过期时间";
}

async function refreshStatus() {
  const run = ++statusRun;
  const [infoResult, tokenResult, webResult] = await Promise.allSettled([
    native("helper-info"), native("helper-status"), native("web-session-status")
  ]);
  if (run !== statusRun) return;
  const info = infoResult.status === "fulfilled" ? infoResult.value : null;
  const token = tokenResult.status === "fulfilled" ? tokenResult.value : null;
  const web = webResult.status === "fulfilled" ? webResult.value : null;
  if (info?.type === "info") {
    helperStatus.textContent = `${info.name} ${info.version} · ${info.target} · ${(info.capabilities || []).join(", ")}`;
    setBadge(helperBadge, "已连接", "ok");
  } else {
    helperStatus.textContent = infoResult.reason?.message || "helper 未安装、未注册或暂时无响应";
    setBadge(helperBadge, "不可用", "warn");
  }
  if (web?.type === "web_session_status") {
    webStatus.textContent = web.logged_in ? "已检测到 bilibili.com Cookie" : "未检测到完整网页 Cookie";
  } else webStatus.textContent = "无法读取网页 Cookie";
  if (token?.type === "status") {
    accountStatus.textContent = token.configured
      ? `${formatExpiry(token.expires_at)} · 刷新 token：${token.has_refresh_token ? "可用" : "没有"}`
      : "未配置 Android 登录态";
    setBadge(accountBadge, token.configured ? "已配置" : "未配置", token.configured ? "ok" : "warn");
  } else {
    accountStatus.textContent = tokenResult.reason?.message || "无法读取 Android 登录态";
    setBadge(accountBadge, "检查失败", "warn");
  }
}

function normalizeTokenText(raw) {
  const text = String(raw || "").trim();
  if (!text) throw new Error("剪贴板为空");
  try {
    const value = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("JSON 必须是对象");
    return JSON.stringify(value);
  } catch (_) {
    if (/^[A-Za-z0-9._~-]{16,256}$/.test(text)) return JSON.stringify({ access_key: text });
    throw new Error("剪贴板内容不是 token JSON 或 Android access key");
  }
}

async function importFromClipboard() {
  let text;
  try { text = await navigator.clipboard.readText(); }
  catch (_) { throw new Error("Firefox 未允许读取剪贴板，请在首选项页授权后重试"); }
  await native("token-import", { token_json: normalizeTokenText(text) });
  await refreshStatus();
  show("登录态已保存到本机 helper。", "success");
}

document.getElementById("paste-token").addEventListener("click", (event) =>
  runButton(event.currentTarget, "读取中…", importFromClipboard));

document.getElementById("clear-token").addEventListener("click", (event) => runButton(event.currentTarget, "清除中…", async () => {
  if (!confirm("确定清除本机保存的 Android 登录态吗？")) return;
  await native("token-clear"); await refreshStatus(); show("已清除 Android 登录态。", "success");
}));

document.getElementById("refresh-button").addEventListener("click", (event) => runButton(event.currentTarget, "刷新中…", async () => {
  await native("helper-refresh"); await refreshStatus(); show("Android 登录态已刷新。", "success");
}));

document.getElementById("web-cookie-login").addEventListener("click", (event) => runButton(event.currentTarget, "同步中…", async () => {
  show("正在使用当前 bilibili.com 网页登录态换取 Android 登录态……");
  const response = await native("web-cookie-login");
  if (response?.type !== "qr_polled" || response.state !== "authorized") throw new Error(response?.message || "网页登录态未能换取 Android token");
  await refreshStatus(); show("网页登录态已同步为 Android 登录态。", "success");
}));

document.getElementById("helper-refresh").addEventListener("click", (event) => runButton(event.currentTarget, "检查中…", async () => {
  await refreshStatus(); show("Helper 和登录态检查完成。", "success");
}));
document.getElementById("install-helper").addEventListener("click", () => browser.tabs.create({ url: RELEASES_URL }));
document.getElementById("uninstall-helper").addEventListener("click", () => browser.tabs.create({ url: `${INSTALL_DOC_URL}#uninstall` }));

document.getElementById("qr-cancel").addEventListener("click", () => {
  qrPolling = false; qrBox.hidden = true; show("已取消二维码登录。");
});

document.getElementById("qr-login").addEventListener("click", (event) => runButton(event.currentTarget, "申请中…", async () => {
  qrPolling = true; qrBox.hidden = true; show("正在申请 Android 登录二维码……");
  const started = await native("qr-start");
  if (started?.type !== "qr_started" || !started.qr_svg) throw new Error("二维码生成失败");
  qrImage.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(started.qr_svg)}`;
  qrBox.hidden = false; show("等待手机确认（最多 90 秒）……");
  const deadline = Date.now() + 90_000;
  while (qrPolling && Date.now() < deadline) {
    await sleep(2_000);
    if (!qrPolling) return;
    const polled = await native("qr-poll");
    if (polled?.state === "authorized") {
      const buvid = await browser.cookies.get({ url: "https://www.bilibili.com/", name: "buvid3" });
      if (buvid?.value) await native("set-buvid", { buvid: buvid.value });
      qrPolling = false; qrBox.hidden = true; await refreshStatus(); show("Android 登录成功，访问密钥已保存到本机 helper。", "success"); return;
    }
    if (polled?.state === "expired") throw new Error("二维码已过期，请重新申请");
  }
  qrPolling = false; qrBox.hidden = true;
  if (Date.now() >= deadline) throw new Error("二维码等待超时，请重新申请");
}));

refreshStatus().then(() => show("就绪。", "success")).catch((error) => show(`初始化失败：${error.message}`, "error"));
