# Firefox extension

这是 Firefox 网页播放器适配扩展：

- 页面侧拦截 UGC `/x/player/wbi/playurl` 及会员状态接口。
- 只有 helper 返回明确的规范化 JSON 才替换官方响应。
- helper 不可用时自动回退到官方网页响应。
- token 通过 Android QR 或首选项剪贴板导入发送到 Native Messaging helper，不回显完整 secret。

Native host 名称：`com.biliwebandroidstream.helper`。

Native host 安装包和跨平台安装脚本见 GitHub Releases；首选项页的 Helper
区域可打开发布页、检查版本和查看安装/卸载说明。
