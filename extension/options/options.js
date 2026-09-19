const accountStatus = document.getElementById("account-status");
const accountBadge = document.getElementById("account-badge");
const status = document.getElementById("status");
const qrBox = document.getElementById("qr-box");
const qrImage = document.getElementById("qr-image");
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
      chrome.runtime.sendMessage({ type, ...payload }),
      timeout(NATIVE_TIMEOUT_MS, "请求没有响应")
    ]);
  } catch (error) {
    throw new Error(error?.message || String(error));
  }
  if (response?.type === "error") throw new Error(response.message || response.code || "请求失败");
  if (response?.ok === false || response?.code && response?.type !== "status") {
    throw new Error(response.message || response.code || "请求失败");
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
  const tokenResult = await Promise.allSettled([native("helper-status")]);
  if (run !== statusRun) return;
  const token = tokenResult[0].status === "fulfilled" ? tokenResult[0].value : null;
  if (token?.type === "status") {
    accountStatus.textContent = token.configured
      ? `${formatExpiry(token.expires_at)} · 刷新 token：${token.has_refresh_token ? "可用" : "没有"}`
      : "未配置 Android 登录态";
    setBadge(accountBadge, token.configured ? "已配置" : "未配置", token.configured ? "ok" : "warn");
  } else {
    accountStatus.textContent = tokenResult[0].reason?.message || "无法读取 Android 登录态";
    setBadge(accountBadge, "检查失败", "warn");
  }
}

document.getElementById("clear-token").addEventListener("click", (event) => runButton(event.currentTarget, "清除中…", async () => {
  if (!confirm("确定清除本机保存的 Android 登录态吗？")) return;
  await native("token-clear"); await refreshStatus(); show("已清除 Android 登录态。", "success");
}));

document.getElementById("refresh-button").addEventListener("click", (event) => runButton(event.currentTarget, "刷新中…", async () => {
  await native("helper-refresh"); await refreshStatus(); show("Android 登录态已刷新。", "success");
}));


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
      const buvid = await chrome.cookies.get({ url: "https://www.bilibili.com/", name: "buvid3" });
      if (buvid?.value) await native("set-buvid", { buvid: buvid.value });
      qrPolling = false; qrBox.hidden = true; await refreshStatus(); show("Android 登录成功，访问密钥已保存到扩展存储。", "success"); return;
    }
    if (polled?.state === "expired") throw new Error("二维码已过期，请重新申请");
  }
  qrPolling = false; qrBox.hidden = true;
  if (Date.now() >= deadline) throw new Error("二维码等待超时，请重新申请");
}));

refreshStatus().then(() => show("就绪。", "success")).catch((error) => show(`初始化失败：${error.message}`, "error"));
