# BiliWebAndroidStream

让 Firefox 网页播放器改用 Bilibili Android 端播放链路的项目：Firefox 扩展拦截
网页 playurl 请求，本机 Rust Native Messaging helper 走 Android `PlayViewUnite`
gRPC 拿到带签名的 DASH 流，扩展再把安卓流拼接进官方网页响应交给播放器。

已在 Firefox Nightly（Linux）上端到端验证：1080P 高码率、1080P 60 帧、4K
（4096×2160）与 8K（7680×4320）均可播放和手动切换。

## 功能

- **画质全开**：画质菜单沿用官方响应（完整描述与大会员角标，含 8K 超高清）；
  实际播放用 Android 流，每次切换只交付目标 representation，网页 ABR 不会把
  手动选择降回 1080P。8K/杜比等 HEVC 画质由 helper 自动声明 HEVC 偏好和
  8K 能力位（`fnval` 0x400）向服务端获取。
- **Android QR 登录**：首选项页本地渲染二维码，手机扫码后 token 存进本机
  helper（用户配置目录，Unix `0600`）；支持剪贴板 token 导入作为回退。
- **自动刷新**：token 过期前自动调用 Passport `/x/passport-login/oauth2/refresh_token`。
- **一键卸载**：首选项页直接删除 helper、native messaging 注册和登录态
  （Windows 上运行中的 exe 会改名后由延迟清理命令删除）。
- **失败回退**：helper 不在或响应异常时，播放器使用官方网页响应，页面可正常播放。

## 安装

1. 从 [GitHub Releases](https://github.com/Ujhhgtg/BiliWebAndroidStream/releases)
   下载 `BiliWebAndroidStream-firefox-vX.Y.Z.zip` 和对应平台的 helper 压缩包。
2. Firefox 打开 `about:addons` → 齿轮 → "从文件安装附加组件" 选择扩展 zip
   （或临时加载用于试用）。
3. 解压 helper 压缩包，运行里面的 `install-native-host.sh`（Windows 为
   `install-native-host.ps1`）注册 Native Messaging host。
4. 打开扩展首选项页，用哔哩哔哩手机客户端扫码完成 Android 登录。
5. （重新）打开 Bilibili 视频页，即可在画质菜单选择高画质。

卸载：在首选项页点击"卸载 Native Helper"，再移除扩展即可，无需运行脚本。

## 构建

```sh
scripts/build.sh        # cargo build --release + 打包扩展到 dist/
scripts/install-native-host.sh   # 注册本机 Native Messaging host
cargo test
```

`scripts/build.sh` 产出 `bin/bili-web-android-stream-helper` 和
`dist/BiliWebAndroidStream-firefox-v<版本>.zip`。CI（`.github/workflows/release.yml`）
在推送 `v*` tag 时构建 Linux/macOS/Windows helper 并与扩展一起发布 Release。

## 架构与协议

- **扩展**（`extension/`）：`page-bridge.js` 在页面内拦截
  `/x/player/wbi/playurl`（XHR 与 fetch），`content-bridge.js` 中转，
  `background.js` 经 Native Messaging 调用 helper；拿到安卓流后把它拼进同一
  请求的官方响应（播放器的 `DashBilibiliParser` 要求每条 representation 携带
  MP4 `segment_base` 字节范围，由 helper 探测填充）。
- **helper**（`src/`）：Firefox Native Messaging 四字节长度帧 + JSON（≤8 MiB）。
  命令：`ping` / `info` / `status` / `set_token` / `clear_token` / `qr_start` /
  `qr_poll` / `refresh_token` / `playback_auth` / `web_cookie_login`（实验性）/
  `resolve_play_url` / `uninstall`。响应不包含密钥字段。

## 隐私

helper 不读取 Firefox profile 数据库，不打印 access key，调试日志脱敏。网页
Cookie 只在用户明确点击"同步当前网页登录态"后，经 Firefox cookies API 取三
项交给本机 helper；token 只存在本机用户配置目录。
