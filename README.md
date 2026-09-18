# BiliWebAndroidStream

将 Bilibili Android 播放请求路径研究并适配到 Firefox 网页播放器的设计阶段项目。

当前仓库包含 Firefox 扩展和 Rust Native Messaging helper 的闭环：

- Firefox Native Messaging 四字节长度帧和 8 MiB 限制。
- Android QR 登录、剪贴板 token 导入、校验和用户配置目录存储（Unix `0600`）。
- `status`、`set_token`、`clear_token`、`resolve_play_url` 协议消息。
- Android `PlayViewUnite` gRPC/protobuf 请求、metadata/device/network headers 和 DASH 响应归一化。
- Firefox 页面侧 UGC `fetch` bridge；helper 不可用时回退官方网页响应。
- 实验性网页登录态授权桥：通过 Firefox cookies API 调官方 Passport
  `auth_code → h5/qrcode/confirm → poll`，并在过期前用
  `/x/passport-login/oauth2/refresh_token` 轮换 token。
- 网页会员状态和 playurl XHR/fetch 适配，使用 Android DASH 流替换网页播放源。

helper 不读取 Firefox profile 数据库、不打印 access key。网页登录态授权桥只在用户点击首选项按钮后，通过 Firefox cookies API 取得三项 Cookie 并交给本机 helper；本地 token JSON 导入仍作为回退流程。

设计稿：[docs/design.md](docs/design.md)

协议草案：[docs/helper-protocol.md](docs/helper-protocol.md)

## 本地检查

```sh
cargo test
cargo check

# 构建并安装 Firefox Native Messaging host
scripts/build.sh
scripts/install-native-host.sh
scripts/package-extension.sh
```

发布版扩展和跨平台 helper 安装包位于 [GitHub Releases](https://github.com/Ujhhgtg/BiliWebAndroidStream/releases)。
安装 helper 后，在 Firefox 首选项页使用 Android QR 登录，重新加载 Bilibili UGC
视频页面，再从画质菜单选择高画质。
