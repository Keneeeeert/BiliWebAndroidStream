const accountStatus = document.getElementById("account-status");
const helperStatus = document.getElementById("helper-status");
const status = document.getElementById("status");
const tokenText = document.getElementById("token-text");
const qrBox = document.getElementById("qr-box");
const qrImage = document.getElementById("qr-image");
const RELEASES_URL = "https://github.com/Ujhhgtg/BiliWebAndroidStream/releases/latest";
const INSTALL_DOC_URL = "https://github.com/Ujhhgtg/BiliWebAndroidStream/blob/main/docs/helper-install.md";

function show(value) { status.textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2); }

async function native(type, payload = {}) {
  const response = await browser.runtime.sendMessage({ type, ...payload });
  if (response?.type === "error") throw new Error(response.message || response.code);
  if (response?.code && response.ok === false) throw new Error(response.message || response.code);
  return response;
}

async function refreshStatus() {
  const [info, token] = await Promise.all([
    native("helper-info").catch((error) => ({ ok: false, message: String(error) })),
    native("helper-status").catch((error) => ({ ok: false, message: String(error) }))
  ]);
  helperStatus.textContent = info?.type === "info"
    ? `${info.name} ${info.version} · ${info.target} · ${info.capabilities.join(", ")}`
    : `helper 未安装或未注册：${info?.message || "无法连接"}`;
  if (token?.type === "status") {
    const expiry = token.expires_at ? new Date(token.expires_at * 1000).toLocaleString() : "未知";
    accountStatus.textContent = token.configured
      ? `Android 登录态已配置 · 过期时间：${expiry} · 可刷新：${token.has_refresh_token ? "是" : "否"}`
      : "未配置 Android 登录态";
  } else accountStatus.textContent = "无法读取登录态；请先安装并注册 helper";
}

function normalizeTokenText(raw) {
  const text = raw.trim();
  if (!text) throw new Error("没有可导入内容");
  try {
    const value = JSON.parse(text);
    if (!value || typeof value !== "object") throw new Error("JSON 必须是对象");
    return JSON.stringify(value);
  } catch (_) {
    if (/^[A-Za-z0-9._~-]{16,256}$/.test(text)) return JSON.stringify({ access_key: text });
    throw new Error("请输入 token JSON 或访问密钥");
  }
}

async function importToken(raw) {
  await native("token-import", { token_json: normalizeTokenText(raw) });
  tokenText.value = "";
  await refreshStatus();
  show("登录态已保存到本机 helper。");
}

document.getElementById("paste-token").addEventListener("click", async () => {
  try { await importToken(await navigator.clipboard.readText()); }
  catch (error) { show(`剪贴板导入失败：${error.message}`); }
});
document.getElementById("import-token").addEventListener("click", async () => {
  try { await importToken(tokenText.value); }
  catch (error) { show(`导入失败：${error.message}`); }
});
document.getElementById("clear-token").addEventListener("click", async () => {
  if (!confirm("确定清除本机保存的 Android 登录态吗？")) return;
  try { await native("token-clear"); await refreshStatus(); show("已清除本机登录态。"); }
  catch (error) { show(`清除失败：${error.message}`); }
});
document.getElementById("refresh-button").addEventListener("click", async () => {
  try { await native("helper-refresh"); await refreshStatus(); show("Android 登录态已刷新。"); }
  catch (error) { show(`刷新失败：${error.message}`); }
});
document.getElementById("helper-refresh").addEventListener("click", () => refreshStatus().catch((error) => show(error.message)));
document.getElementById("install-helper").addEventListener("click", () => browser.tabs.create({ url: RELEASES_URL }));
document.getElementById("uninstall-helper").addEventListener("click", () => browser.tabs.create({ url: `${INSTALL_DOC_URL}#uninstall` }));

document.getElementById("qr-login").addEventListener("click", async () => {
  try {
    qrBox.hidden = true;
    show("正在申请 Android 登录二维码……");
    const started = await native("qr-start");
    if (started?.type !== "qr_started" || !started.qr_svg) throw new Error("二维码生成失败");
    qrImage.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(started.qr_svg)}`;
    qrBox.hidden = false;
    show("等待手机确认登录……");
    for (;;) {
      await new Promise((resolve) => setTimeout(resolve, 2000));
      const polled = await native("qr-poll");
      if (polled?.state === "authorized") {
        const buvid = await browser.cookies.get({ url: "https://www.bilibili.com/", name: "buvid3" });
        if (buvid?.value) await native("set-buvid", { buvid: buvid.value });
        await refreshStatus();
        show("Android 登录成功，访问密钥已保存到本机 helper。");
        return;
      }
      if (polled?.state === "expired") throw new Error("二维码已过期，请重新申请");
    }
  } catch (error) { show(`QR 登录失败：${error.message}`); }
});

refreshStatus().catch((error) => show(`初始化失败：${error.message}`));
