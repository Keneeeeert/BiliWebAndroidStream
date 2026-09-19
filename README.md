# BiliWebAndroidStream

让 Firefox 网页播放器改用 Bilibili Android 端播放链路的项目：Firefox 扩展在
后台页内直接走 Android `PlayViewUnite` gRPC 拿到带签名的 DASH 流，再把安卓流
拼接进官方网页响应交给播放器。

已在 Firefox（Linux）上端到端验证：1080P 高码率、1080P 60 帧、4K
（4096×2160）与 8K（7680×4320）均可播放和手动切换。

## 功能

- **画质全开**：画质菜单沿用官方响应（完整描述与大会员角标，含 8K 超高清）；
  实际播放用 Android 流，每次切换只交付目标 representation，网页 ABR 不会把
  手动选择降回 1080P。8K/杜比等 HEVC 画质由扩展自动声明 HEVC 偏好和 8K
  能力位（`fnval` 0x400）向服务端获取。
- **Android QR 登录**：首选项页本地渲染二维码，手机扫码后 token 存进扩展
  本地存储；除哔哩哔哩官方接口外不向任何服务器发送。
- **自动刷新**：token 过期前自动调用 Passport `/x/passport-login/oauth2/refresh_token`。
- **失败回退**：gRPC 响应异常时，播放器使用官方网页响应，页面可正常播放。

## 安装

### Firefox

1. 从 [GitHub Releases](https://github.com/Ujhhgtg/BiliWebAndroidStream/releases)
   下载 `BiliWebAndroidStream-vX.Y.Z-signed.xpi`。
2. 把 xpi 拖进 Firefox 窗口，或 `about:addons` → 齿轮 → "从文件安装附加
   组件" 选择该 xpi。

### Chrome / Chromium

1. 下载 `BiliWebAndroidStream-chrome-vX.Y.Z.zip` 并解压（未上架 Chrome 商店，
   需以开发者模式加载）。
2. 打开 `chrome://extensions` → 开启"开发者模式" → "加载已解压的扩展程序"
   选择解压出的目录。

打开扩展首选项页，用哔哩哔哩手机客户端扫码完成 Android 登录，然后（重新）
打开 Bilibili 视频页，即可在画质菜单选择高画质。

## 构建

```sh
scripts/build.sh        # 打包两个浏览器扩展到 dist/
```

产出 `dist/BiliWebAndroidStream-v<版本>-unsigned.zip`（Firefox）与
`dist/BiliWebAndroidStream-chrome-v<版本>.zip`（Chrome，构建时把后台脚本
拼接成 MV3 service worker）。CI（`.github/workflows/release.yml`）在推送
`v*` tag 时校验语法、核对版本号并发布 Release：无签名包直接挂到 Release，
Firefox 包同时上传 AMO unlisted 渠道自动签名，签名完成后另一个工作流把
`-signed.xpi` 挂回同一个 Release。扩展无构建依赖，包即源码。

## 架构与协议

- **page-bridge.js**（注入页面）：拦截 `/x/player/wbi/playurl`（XHR 与
  fetch），拿到安卓流后把它拼进同一请求的官方响应——播放器的
  `DashBilibiliParser` 要求每条 representation 携带 MP4 `segment_base`
  字节范围，由后台向 CDN 发 Range 请求探测填充。
- **content-bridge.js**（content script）：页面与后台之间的消息中转。
- **background.js + native-api.js**（后台页）：TV 档扫码登录（`passport-tv-login/qrcode`）、
  oauth2 token 刷新（TV appkey 档参数 + `ts`）、`PlayViewUnite` gRPC
  （`grpc.biliapi.net`，5 字节 gRPC 帧 + base64 protobuf 元数据头，
  `identify_v1 <access_key>` 授权）；请求参数签名即
  `md5(排序后的 urlencode query + appsec)`。token 存
  `browser.storage.local`。

## 隐私

扩展不读取 Firefox profile 数据库，不打印 access key。token 只存在本机扩展
存储里，除哔哩哔哩官方接口（passport 与 gRPC 播放接口）外不向任何服务器发送。
