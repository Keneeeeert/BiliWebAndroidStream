# Android access key 链路分析

## 结论

CI 实际下载的宿主 APK 地址在 [ci-apk.yml](/home/ujhhgtg/coding/BiliRoamingX/.github/workflows/ci-apk.yml:37)，完整 APK 已用 apktool 低内存模式解码；关键 Passport smali 已保留在 `/tmp/bilibili-host-passport-smali`。此前只看集成 APK 得出的结论不完整；完整宿主 APK 确实包含 Passport 登录实现。

因此现在可以还原登录、二维码、刷新和落盘链路；仍不会读取或输出用户真实 token。

## 集成层读取链路

`Accounts`/反编译后的 `w0` 做了以下事情：

1. 优先读取宿主 `files/bili.passport.storage` 和 `files/bili.account.storage`。
2. 迁移后的账号状态来自 `controller.blkv` 的 `current_account.token`/`cookie` 字段。
3. token 文件内容是 Base64，再用宿主 `Utils.getAppKey()` 的前 16 字节作为 AES-CBC/PKCS5 密钥，并用 `Utils.getAesIv(appKey)` 作为 IV 解密。
4. 解密后的 token JSON 字段包括：
   - `access_token`（运行时作为 `accessKey`）
   - `refresh_token`
   - `expires` / `expire_in`
   - `fast_login_token`
   - `mid`
5. Cookie 单独存储为 CookieInfo，常见字段是 `SESSDATA`、`bili_jct` 等。
6. 宿主通过 `com.bilibili.passport.ACTION_MSG` 广播账号变化；集成层对登出、更新、切换事件清理缓存。

对应材料：

- [Accounts.kt](/home/ujhhgtg/coding/BiliRoamingX/integrations/app/src/main/java/app/revanced/bilibili/account/Accounts.kt)
- [反编译 w0.java](/tmp/bili-apk-src/sources/kofua/w0.java)
- [AccessToken.kt](/home/ujhhgtg/coding/BiliRoamingX/integrations/app/src/main/java/app/revanced/bilibili/account/model/AccessToken.kt)
- [反编译 ToolFragment.java](/tmp/bili-apk-src/sources/app/revanced/bilibili/settings/fragments/ToolFragment.java)

## 宿主登录链路的结构

完整宿主 APK 的 [BiliAuthService.smali](/tmp/bilibili-host-passport-smali/BiliAuthService.smali) 明确声明了这些接口：

- `/x/passport-tv-login/qrcode/auth_code`
- `/x/passport-tv-login/qrcode/poll`
- `/x/passport-login/oauth2/login`
- `/x/passport-login/oauth2/access_token`
- `/x/passport-login/oauth2/refresh_token`
- `/x/passport-login/fast/login`
- `/x/passport-login/login/sms`
- `/x/passport-login/login/intl/sns`
- `/x/passport-login/web/key`

`BiliAccounts.qrCodeAuthCode()` 获取二维码，`BiliAccounts.qrCodePoll()` 轮询成功后调用内部账号保存逻辑，把 `AuthInfo` 写入账号存储并通知登录状态。[BiliAccounts.smali](/tmp/bilibili-host-passport-smali/BiliAccounts.smali:4351)

`BiliAuthService.refreshTokenV2()` 使用：

```text
POST /x/passport-login/oauth2/refresh_token
fields: access_key, refresh_token, sts, device parameters
```

账号密码/手机验证登录还涉及：

- Passport 公钥接口；
- RSA 加密密码或设备临时密钥；
- AES 加密设备 metadata；
- Android appkey/appsec 签名；
- `bili_local_id`、buvid、设备信息、游客 ID 和风控字段；
- 登录成功后返回 token_info 和 cookie_info；
- refresh 使用 `oauth2/refresh_token`，提交现有 access token、refresh token 和 app 签名。

这条链路依赖客户端 app secret、设备状态和风控验证，不能安全地在网页扩展里硬编码或模拟完整密码登录。

## 可自动化的候选：TV/APP 二维码授权

公开接口记录了另一条候选链路，且宿主 APK 也直接声明了这两个 TV/APP QR endpoint：

1. `POST /x/passport-tv-login/qrcode/auth_code`
2. 展示返回的二维码，用户使用 Bilibili 手机端扫码确认。
3. 轮询 `POST /x/passport-tv-login/qrcode/poll`。
4. 成功响应包含 `mid`、`access_token`、`refresh_token`、`expires_in`，有时还返回 token_info/cookie_info。

网页二维码接口则主要建立 SESSDATA 等网页 Cookie，不直接返回 Android access token，因此不能直接用于本项目的 gRPC 身份。

这个 TV/APP QR 流程已加入 helper 的实验桥，扩展只在用户点击按钮后读取三项网页 Cookie 并交给本机 helper。成功响应按宿主 APK 的 `data.token_info` 结构解析，并在访问密钥临近过期时走宿主同名 refresh endpoint。真实高画质 entitlement 仍未验证。需要继续验证：

- 当前 TV endpoint 和 appkey 签名仍有效；
- TV 返回的 token 能否用于 Android `PlayViewUnite`；
- 返回 token 是否能让普通 UGC 响应包含目标质量的 DASH URL；
- token 刷新和撤销是否稳定；
- appkey/appsec 的使用是否符合接口授权范围。

如果验证失败，首选项页应继续使用“从 Android 客户端复制 access key 后导入 JSON”的流程。

## 对项目的影响

当前 helper 已实现本地 token JSON 导入和用户配置目录 `0600` 存储。实验桥只通过 Firefox cookies API 读取当前网页会话，不访问 SQLite 数据库，不解密 Android 私有文件，也不把 token 发到第三方服务器。

推荐后续顺序：用用户明确授权的真实会话验证 QR 换出的 token 是否能让 `PlayViewUnite` 返回高画质流；在验证前，QR 桥必须保持“实验性”标记，手动 token 导入继续保留。
