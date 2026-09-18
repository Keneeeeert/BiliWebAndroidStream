# BiliWebAndroidStream 设计稿

状态：第一版扩展/helper 已实现；真实 Android access key 下的高画质 entitlement 验证仍待进行。

当前实现范围：

- Rust helper 已能读取/持久化 token JSON、构造 `PlayViewUnite` protobuf、发送 gRPC 请求、解析首个 response frame，并归一化 DASH video/audio。
- Firefox 扩展已提供 options 页面、Native Messaging bridge 和 UGC `fetch` bridge。
- helper 返回的流数据会转换为网页播放器使用的 JSON 形状；helper 不可用、响应没有目标质量或请求失败时，页面继续使用官方响应。
- 当前没有真实 token 测试，因此还没有宣称高画质一定可用。

## 已确认的用户决策

- 运行形态：Firefox 扩展 + 本地 Native Messaging helper。
- 首版范围：只支持普通 UGC 视频。
- helper 语言：Rust。
- 首个验证目标：直接接入 Bilibili 网页播放器。
- 凭据输入：首选项页支持导入 Android token JSON 文件。
- 登录/凭据：首选项页应优先尝试自动化登录和 access key 获取；如果没有稳定、可验证的官方流程，则改为本地手动导入/填写，并给出指导。

## 目标

实现一个 Firefox 网页端适配层，使网页播放器能够使用 Bilibili Android 播放接口返回的 DASH 播放数据，并复用 Android 客户端对高画质试看流的本地处理逻辑。

首个目标是普通 UGC 视频。项目不应伪造或上传用户凭据，也不依赖第三方远程代理；用户明确导入的 token 仅保存到本机 helper 配置目录。

## 已确认的 Android 行为

仓库中的 ReVanced 集成和 APK 反编译结果一致：

- 新版普通视频主要走 `bilibili.app.playerunite.v1.Player/PlayViewUnite`。
- 旧播放器走 `bilibili.app.playurl.v1.PlayURL/PlayView`。
- 两个接口都由 `grpc.biliapi.net` 提供，使用 unary gRPC + protobuf。
- 新接口在非有效 VIP 且开关打开时，把 `VideoVod.is_need_trial`（字段号 11）设为 `true`。
- 播放请求的 `VideoVod` 同时携带 `qn`、`fnval`、`fourk` 等字段；Android 补丁会使用 `fnval=4048`、`fourk=true` 来请求 DASH/高编码/高画质能力。
- 新接口响应中的 `VodInfo.stream_list` 已包含 DASH `base_url`/`backup_url` 时，补丁只把 `StreamInfo.need_vip` 改为 `false`、`vip_free` 改为 `true`，并清除 `QnTrialInfo`。
- 旧接口响应同样只修改已有 DASH 流标志，但清除的是 `PlayViewReply.ab`。`AB.Glance` 包含 `can_watch`、`duration`、`times`，对应本地试看门控。
- 代码没有生成 URL、替换分片、截断视频或改变视频时长。因此“全视频”来自服务端已经返回的完整 DASH URL，客户端限制主要由 `AB`、`QnTrialInfo` 和 `need_vip` 元数据触发。

关键材料：

- [TrialQualityPatch.java](/home/ujhhgtg/coding/BiliRoamingX/integrations/app/src/main/java/app/revanced/bilibili/patches/TrialQualityPatch.java)
- [PlayURLPlayViewUGC.kt](/home/ujhhgtg/coding/BiliRoamingX/integrations/app/src/main/java/app/revanced/bilibili/patches/protobuf/hooks/PlayURLPlayViewUGC.kt)
- [BangumiPlayUrlHook.kt](/home/ujhhgtg/coding/BiliRoamingX/integrations/app/src/main/java/app/revanced/bilibili/patches/protobuf/BangumiPlayUrlHook.kt)
- [MossPatch.kt](/home/ujhhgtg/coding/BiliRoamingX/integrations/app/src/main/java/app/revanced/bilibili/patches/protobuf/MossPatch.kt)
- [反编译 z6.java](/tmp/bili-apk-src/sources/kofua/z6.java)
- [反编译 vd.java](/tmp/bili-apk-src/sources/kofua/vd.java)
- [grpc_apis.jar](/home/ujhhgtg/coding/BiliRoamingX/integrations/dummy/libs/grpc_apis.jar)

## 已确认的网页行为

当前网页播放器核心代码 [core.js](https://s1.hdslb.com/bfs/static/player/main/core.06f1e939.js) 使用 REST 播放接口：

- UGC：`/x/player/wbi/playurl`
- PGC：`/pgc/player/web/v2/playurl`

网页请求和 Android gRPC 请求不是同一协议。网页端普通视频的 `try_look` 参数也不能直接等价替代 Android protobuf 中的 `is_need_trial`。

使用本机 Firefox profile 的临时副本进行的观察：

- 普通 UGC 网页请求通常只返回当前账号可用的 DASH 质量；测试视频返回过 `32/16`，请求 `qn=112` 后仍回退到低画质。
- PGC 网页接口在部分剧集上可以返回高画质 DASH 地址和 `qn_trial_info`，但这不能证明 UGC 也走相同策略。
- 网页页面和 `grpc.biliapi.net` 之间没有可供页面 `fetch` 使用的 CORS 授权；gRPC 还需要二进制 framing、protobuf 和 trailers。

## 协议和凭据要求

Android 请求至少需要：

- `authorization: identify_v1 <access_key>`
- `x-bili-metadata-bin`：protobuf `Metadata`，包含 access key、`mobi_app=android`、build、platform、buvid 等
- `x-bili-device-bin`：protobuf `Device`，包含 app/build、设备、系统和指纹字段
- `x-bili-network-bin`：protobuf `Network`；Android 补丁在统一播放请求中把 `type` 设为 Wi-Fi
- gRPC 请求路径和 protobuf method descriptor

网页里的 `SESSDATA` 不能直接当作 Android `access_key`。当前没有从 Firefox profile 读取或输出任何凭据，也没有确认“只凭网页 Cookie”能获得 Android 同等 entitlement。

用空/伪造 Android 鉴权探测 gRPC 时，服务可以返回 protobuf 响应，但高画质流仍只有 `need_vip=true` 且没有 DASH URL。这说明 `is_need_trial=true` 本身不足以获得普通视频高画质，真实 Android access key、设备 metadata 和服务端策略仍是未验证前提。

## 推荐架构

```text
Firefox content/page adapter
        │
        │ localhost IPC / Native Messaging
        ▼
Local Android-playback helper
        │
        │ unary gRPC + protobuf + Android metadata
        ▼
grpc.biliapi.net
        │
        ▼
Normalize PlayViewUniteReply / PlayViewReply
        │
        ├─ retain full DASH video/audio URLs
        ├─ remove qn trial/client gate metadata locally
        └─ return web-player media source
```

### Firefox 侧

负责：

- 识别当前 BVID、AID、CID 和目标画质。
- 在播放器请求前调用本地 helper。
- 接收规范化后的播放数据。
- 将视频/音频 DASH 数据交给网页播放器的 MSE/DASH 数据源。
- 处理播放器缓存、画质切换和失败回退。

不负责：

- 直接向 gRPC 端点发送 Android 请求。
- 读取 Firefox 数据库中的 access key。
- 把凭据发送到第三方服务。

### 本地 helper

负责：

- protobuf 编解码。
- gRPC 5-byte message framing 和 HTTP/2 传输。
- 构造 `Metadata`、`Device`、`Network`。
- 使用用户明确提供并本地保存的 Android access key。
- 解析 `PlayViewUniteReply` / `PlayViewReply`。
- 将 `VodInfo.stream_list` 或 `VideoInfo.stream_list` 转换成网页侧稳定 JSON。

helper 已选择 Rust；protobuf/gRPC 代码和 Native Messaging 边界已在 helper 中实现。

## 为什么不直接改网页 JSON

只改网页端 `need_vip`、`accept_quality` 或画质菜单会产生假选项。普通 UGC 如果响应里没有目标质量的 DASH `base_url`，浏览器没有可播放的资源。

真正需要验证的是：使用合法且用户明确授权的 Android access key 和真实设备 metadata 调用 Android `PlayViewUnite` 后，普通 UGC 的目标质量是否出现带 `base_url` 的 stream。这个实验尚未完成，是实现前的关键阻塞点。

## 方案选择

### 方案 A：Firefox 扩展 + Native Messaging helper（推荐）

优点：

- 页面逻辑和 gRPC 网络逻辑隔离。
- 不需要向页面暴露 access key。
- helper 可以使用成熟的 HTTP/2、protobuf 库。
- 后续可以把 Android response 转成稳定的网页侧接口。

代价：

- 需要安装扩展和本地 host manifest。
- 需要处理 helper 生命周期、版本兼容和错误提示。
- 需要明确 access key 的本地来源和保存方式。

### 方案 B：本地 HTTP 代理

优点：页面只连接 localhost，播放器改动较少。

代价：

- 必须处理跨域、端口、进程安全和 URL 签名。
- 容易误把 access key 暴露给浏览器页面。
- 不适合作为默认方案。

### 方案 C：纯 Userscript

不作为主方案。Userscript 可以改页面已有 JSON 和 UI，但不能可靠地发 Android gRPC，也不能解决 Android access key、设备 metadata 和 CORS。

## 剩余验证问题

以下问题不会改变当前架构，但会影响自动登录和 entitlement 是否可以宣称可用：

1. **凭据验证**：需要用户明确授权的真实会话，通过扩展实验桥或本地 token 文件验证 Android gRPC 高画质 entitlement。不要在聊天中发送 secret。
2. **流交付方式**：已确认首个验证目标直接接入 Bilibili 网页播放器；在网页播放器接入失败时，再降级为独立 DASH 诊断页。

### 登录和 access key 自动化调查结论

完整宿主 APK 已确认 Android Passport 的二维码和刷新接口。公开第三方实现还验证了“网页 Cookie → APP token”的实验桥：申请 `auth_code`，带网页 Cookie 调用 `h5/qrcode/confirm`，再签名轮询 `qrcode/poll`。Android 账户模型把 `access_token` 保存为 `accessKey`，同时配套 `refresh_token`、`mid` 和过期时间；这属于 Android passport/token 状态，不是网页 Cookie 的同义字段。

因此首版不自动读取 Firefox SQLite 数据库或 Android 应用私有文件，也不把 access key 发送给第三方服务。首选项页提供两级流程：

1. **实验性自动化**：用户点击后通过 Firefox cookies API 读取当前 `bilibili.com` 的三项 Cookie，交给本机 helper 调官方 Passport confirm/poll；不注入密码、不访问 Cookie SQLite 文件、不上传第三方。
2. **手动导入回退**：用户在自己的 Android 客户端或授权工具中获取 access key/token 文件，在首选项页选择本地文件导入。helper 只在本机保存最小必要字段，并提供清除按钮；页面不回显完整 secret。

自动化桥已加入项目，但仍标记为实验性；它使用公开资料中的 appkey/appsec，可能随服务端变化，且真实高画质 entitlement 尚未验证。

用户已选择把规范化 token 存在 helper 的用户配置目录，并在 Unix 上设置 `0600` 权限。helper 的清除命令会删除该文件；扩展首选项页不会回显完整 secret。

Firefox 的扩展权限和 Native Messaging 机制本身是可行的：扩展后台页面可以通过 host permissions 发跨域请求，Native Messaging 通过本机 manifest 和 stdin/stdout 交换 JSON；内容脚本不能直接使用 Native Messaging，必须经由后台脚本转发。参考 [MDN host_permissions](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/host_permissions) 和 [MDN Native messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging)。

### APK access key 链路的精确结论

当前仓库中的 `app-release.apk` 是 BiliRoamingX 集成 APK，不是完整的 Bilibili 宿主 APK。因此它没有登录页面、二维码生成或 Passport 登录实现；它只观察宿主的账号存储和 Passport 广播。

反编译代码显示：

- `w0.i()` 从宿主私有文件 `files/bili.passport.storage` 读取加密 token，从 `files/bili.account.storage` 读取 cookie。
- token 先 Base64 解码，再使用 `Utils.getAppKey()` 派生 AES-CBC/PKCS5 密钥和 IV，解密后按 `AccessToken` JSON 解析。
- `AccessToken` 字段是 `access_token`、`refresh_token`、`expires`、`expire_in`、`fast_login_token`、`mid`；运行时把 `access_token` 当作 `accessKey` 使用。
- 账号变更通过 `com.bilibili.passport.ACTION_MSG` 广播通知，`what=2/5/6` 分别触发登出、账号更新、账号切换缓存刷新。
- BiliRoamingX 的“复制访问密钥”只是把 `w0.c()` 当前 token 的 access key 放入剪贴板，不参与登录。
- 自定义访问密钥设置只是覆盖取流请求使用的 access key，也没有登录能力。

这意味着 Firefox 不应尝试解密 `bili.passport.storage`：需要 Android 宿主的包名、app key、密钥派生实现和私有文件访问权限，而且这仍不能保证 token 与当前设备 metadata 匹配。

公开资料中存在一个可能的自动化入口：TV/APP 二维码流程先申请 `auth_code`，再轮询 `x/passport-tv-login/qrcode/poll`，成功响应包含 `access_token`、`refresh_token`、`mid` 和有效期。[公开接口记录](https://github.com/BACNext/bilibili-API-collect-backup/blob/master/docs/login/login_action/QR.md#tv端扫码登录) 说明该流程要求 TV appkey、签名、`local_id` 和时间戳；它不是网页二维码登录流程，网页二维码只设置 SESSDATA 等 Cookie。[网页二维码流程记录](https://github.com/BACNext/bilibili-API-collect-backup/blob/master/docs/login/login_action/QR.md#web端扫码登录) 也只描述 Cookie/refresh_token。

当前不能把 TV appkey 流程直接承诺为本项目登录方案，原因是：

1. 需要确认当前接口和 appkey 仍可用；
2. 需要确认 TV 返回的 token 能否用于 Android Pink `PlayViewUnite` 高画质 entitlement；
3. 需要实现 appkey 签名和二维码 UI，并处理风控/失效码；
4. 不能把 Android app secret 硬编码到扩展或公开仓库。

因此自动登录已作为独立实验功能加入首选项页：扩展通过 Firefox cookies API 读取当前 `bilibili.com` 的 `SESSDATA`、`DedeUserID`、`bili_jct`，只在内存中发送给本机 helper；helper 执行 `auth_code → h5/qrcode/confirm → poll`，成功后把返回 token 写入本地 0600 文件。该流程使用公开资料中的 783 Android appkey/appsec，可能随服务端变化，且尚未用真实用户会话验证高画质 entitlement。当前仍保留从 Android 客户端复制 access key 后导入 JSON 的回退流程。

## 当前结论

项目技术上存在可行路径，但关键不在网页 UI，而在 Android gRPC 请求能否用真实、合法、用户授权的 access key 和设备 metadata 为普通 UGC 返回带完整 DASH URL 的高画质 stream。

在这个关键实验完成前，不应开始编写浏览器绕过逻辑，也不应假设 `is_need_trial=true` 单独足够。
