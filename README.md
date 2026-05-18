# Celadon

[![Release](https://img.shields.io/github/v/release/MageGojo/Celadon?color=7ea388)](https://github.com/MageGojo/Celadon/releases)
[![License](https://img.shields.io/badge/license-GPL--3.0%20%2F%20Apache--2.0-7a5f52)](./LICENSE-GPL)
[![Platform](https://img.shields.io/badge/platform-macOS-506956)](https://github.com/MageGojo/Celadon)
[![Based on Zed](https://img.shields.io/badge/based%20on-Zed-7a6f89)](https://github.com/zed-industries/zed)

> [English](./README.en.md) · 中文

**Celadon** 是 [Zed 编辑器](https://github.com/zed-industries/zed) 的个人定制分支，在保留 Zed 全部原生能力的基础上，添加了更贴近东方审美的视觉主题与若干本地化功能增强。

项目名 *Celadon*（青瓷）取自宋代瓷器的低饱和釉色，也是项目中 MorandiGarden 主题的主色调。

---

## 功能特性

### 1. MorandiGarden 预览主题

Markdown（`.md`、`.markdown`）和 HTML（`.html`、`.htm`）文件的预览采用 **MorandiGarden 莫兰迪花园主题**，CSS 改编自 [Kitsunee-CN/MorandiGarden](https://github.com/Kitsunee-CN/MorandiGarden) 的 Typora 主题。

预览窗口在 macOS 上使用 `WKWebView`（WebKit）渲染，视觉效果接近原生浏览器：

- 浅米色背景（`#FBFDFB`）配暖色调正文
- 六级标题分别采用不同的莫兰迪色（暖棕 → 鼠尾草绿 → 灰紫 → 板岩蓝 → 浅褐 → 暖灰）
- `h1` 含淡色下划分隔线，`h2` 带青瓷绿左边竖线
- 代码块圆角、青瓷背景
- 引用块（blockquote）支持多级嵌套，每级颜色不同
- 任务列表的 checkbox 渲染为圆形 toggle 动画
- 表格使用青瓷色表头，行 hover 高亮

打开方式：`Cmd+Shift+P` → **Preview: Open Preview**，或点击编辑器工具栏的 👁 图标。

### 2. Windsurf AI 集成

内置 **Windsurf 语言模型 Provider**，连接本地 Windsurf 代理服务。Windsurf IDE 在运行时，Celadon 会自动从下面这个路径读取 session 完成认证：

```
~/Library/Application Support/Windsurf/User/globalStorage/
  sparkcore.xinghuo-windsurf/devin-session-cache.json
```

无需手动配置 API Key。Windsurf 订阅中可用的全部模型（Claude、GPT、Gemini、DeepSeek 等）会自动出现在 Celadon 的 AI 面板中。

配置（可选，全部有默认值）：

```json
{
  "language_models": {
    "windsurf": {
      "api_url": "http://localhost:3003/v1"
    }
  }
}
```

也可设置环境变量 `WINDSURF_AUTH_TOKEN` 进行手动认证。

### 3. 中英双语界面

内置 `i18n` 模块，菜单、面板、设置等界面元素支持简体中文与英文切换。

设置方式：

```json
{
  "locale": "zh-CN"
}
```

支持的值：`"en"`（英文，默认）、`"zh-CN"`（简体中文）。

覆盖范围包括：macOS 应用菜单、命令面板、文件菜单、Project Panel 右键菜单、Settings UI、Agent 面板、Dock、Pane 标签等几乎全部主要 UI。

### 4. 工作区背景图片

可以为 Celadon 主窗口设置一张背景图片。设置后，编辑器背景与 gutter 自动降低至 85% 不透明度，让图片透出。

```json
{
  "experimental.theme_overrides": {
    "background.image_file": "/Users/you/Pictures/celadon-wall.png"
  }
}
```

支持本地任意 PNG/JPEG 图片，也可在设置 UI 中可视化配置。

### 5. `.http` / `.rest` 文件运行器

直接在 Celadon 中编辑并运行 HTTP 请求，无需切到 Postman 或 curl。

打开任意 `.http` 或 `.rest` 文件，工具栏会自动显示 ▶️ 运行按钮：

- 内置 [tree-sitter-http](https://github.com/rest-nvim/tree-sitter-http) 语法高亮
- 解析光标所在的请求块
- 一键发送，响应（含 status、headers、body）会以新 buffer 形式展示
- 响应自动按 HTTP 语法着色，支持 HTTP/1.1、HTTP/2、HTTP/3

示例 `api.http`：

```http
### 获取用户列表
GET https://api.example.com/users
Authorization: Bearer YOUR_TOKEN

### 创建用户
POST https://api.example.com/users
Content-Type: application/json

{
  "name": "Alice",
  "email": "alice@example.com"
}
```

光标停在任一 `### ...` 块内，点击运行按钮即可发送对应请求。

---

## 构建

### 先决条件

- Rust（通过 [rustup](https://www.rust-lang.org/tools/install) 安装）
- macOS：Xcode 与 Xcode Command Line Tools
- `cmake`（`brew install cmake`）

### 开发版本（直接运行）

首次构建需下载约 300 MB 的预编译 `libwebrtc.a`。如果自动下载因 TLS 中断失败，可手动下载：

```sh
# 手动下载 libwebrtc
curl -L -o /tmp/webrtc-mac-arm64-release.zip \
  "https://github.com/zed-industries/livekit-rust-sdks/releases/download/webrtc-0001d84-4/webrtc-mac-arm64-release.zip"
mkdir -p ~/webrtc-prebuilt
unzip /tmp/webrtc-mac-arm64-release.zip -d ~/webrtc-prebuilt

# 编译并运行
LK_CUSTOM_WEBRTC=~/webrtc-prebuilt/mac-arm64-release cargo run
```

Intel 芯片 Mac 请把上面的 `arm64` 替换为 `x64`。

### 打包 macOS `.app`（带图标）

安装定制版的 [`cargo-bundle`](https://github.com/zed-industries/cargo-bundle)：

```sh
cargo install cargo-bundle \
  --git https://github.com/zed-industries/cargo-bundle.git \
  --branch zed-deploy
```

构建 `.app` 包：

```sh
LK_CUSTOM_WEBRTC=~/webrtc-prebuilt/mac-arm64-release \
  script/bundle-mac -d -o
```

`-d` 使用 debug build 加快编译，`-o` 自动打开结果。

> **替换图标**：把自己的 PNG 图（建议 1024×1024）覆盖到 `crates/zed/resources/app-icon-dev.png` 与 `crates/zed/resources/app-icon-dev@2x.png` 即可。

---

## 致谢与版权

Celadon 是 [Zed](https://github.com/zed-industries/zed) 的派生作品，**版权与许可完全继承自上游**。Zed 由 **Zed Industries, Inc.** 开发，本仓库不持有 Zed 任何商标或版权。

- 上游许可证：[GPL-3.0-or-later](./LICENSE-GPL) 与 [Apache-2.0](./LICENSE-APACHE)（详见各 crate 内单独的 LICENSE 文件）
- 第三方依赖许可：见 [`script/licenses/zed-licenses.toml`](./script/licenses/zed-licenses.toml)
- MorandiGarden 主题原作者：[Kitsunee-CN](https://github.com/Kitsunee-CN/MorandiGarden)
- tree-sitter-http：[rest-nvim/tree-sitter-http](https://github.com/rest-nvim/tree-sitter-http)

本项目仅作个人使用与学习实验，不进行任何商业分发。所有源代码仍受上述开源许可证约束，所有 Zed 上游的版权声明、`LICENSE-*` 文件、各 crate 的 license 头部均原样保留。

如需贡献代码到 Zed 上游，请前往 [zed-industries/zed](https://github.com/zed-industries/zed) 与官方 [CONTRIBUTING.md](./CONTRIBUTING.md)。
