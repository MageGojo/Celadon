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

### 2. Windsurf AI 多账号管理

内置 **Windsurf 语言模型 Provider**，支持多账号批量添加、额度显示与自动换号，连接本地 Windsurf 代理服务（端口 `3003`）。

#### 先决条件：启动本地代理

Celadon 通过 [WindsurfAPI](https://github.com/AlexStrNik/windsurf-api) 代理与 Windsurf 通信。**首次使用需要先把代理跑起来：**

```sh
# 克隆并启动代理（需要 Node.js ≥ 18）
git clone https://github.com/AlexStrNik/windsurf-api.git
cd windsurf-api
npm install
npm start
# 默认监听 localhost:3003
```

> **注意**：代理依赖本机已安装的 Windsurf IDE 的 `language_server_macos_arm` 二进制。  
> 路径示例：`/Applications/Windsurf.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm`  
> 只要装了 Windsurf，代理就能正常工作。

#### 配置代理地址（可选，默认 `http://localhost:3003/v1`）

```json
{
  "language_models": {
    "windsurf": {
      "api_url": "http://localhost:3003/v1"
    }
  }
}
```

---

#### 多账号管理 UI

打开 **Settings → AI → Windsurf**，面板提供：

**账号列表**

每行账号显示以下信息：
- `★` — 当前 proxy 最近使用的账号（自动标记）
- **邮箱** — 账号邮箱
- **tier** — `pro` (绿色) / `free` (蓝色) / `expired` (灰色)
- **status** — `active` (绿色) / 其他 (黄色)
- **额度** — `67%↓ 84%/wk`（当日剩余 / 本周剩余，颜色：🟢≥50% · 🟡20-50% · 🔴<20%）
- **套餐** — `Trial` / `Pro` 等

**手动换号（Solo Mode）**

点击账号行右侧的 **Use Only** 按钮，进入 Solo Mode：

- 当前账号保留在代理池，其他账号被**暂停**（从代理池移除，但仍在 UI 中显示为灰色）
- 面板顶部出现「Solo Mode」横幅
- 点击 **Auto (Restore All)** 一键恢复所有暂停账号，重新进入轮询模式
- 重新添加不会产生重复账号（自动跳过已在代理池中的邮箱）

**批量操作**

| 按钮 | 功能 |
|------|------|
| `○` / `✓` | 勾选/取消勾选账号 |
| Select All / Deselect All | 全选/全取消 |
| Delete Selected (N) | 批量删除选中账号 |
| Remove Invalid | 批量删除无可用模型（`0 models`）的账号 |
| Auto (Restore All) | Solo Mode 下：恢复所有暂停账号并退出独占模式 |
| Refresh | 立即刷新所有账号额度 |

**添加账号**

- **单个添加**：在 `Email` 和 `Password` 输入框填写后点击 `Add Account`
- **批量粘贴**：将账号列表复制到剪贴板（每行一个，格式见下），点击 `Paste & Add All`

批量粘贴格式（两种均支持）：
```
# 格式 1：----分隔
email1@example.com----password1
email2@example.com----password2

# 格式 2：:分隔
email1@example.com:password1
email2@example.com:password2
```

**自动换号**

- 代理本身具备 **round-robin 负载均衡**，当某个账号额度耗尽或出错时，代理自动切换到下一个可用账号
- Celadon 每 **5 分钟**自动刷新一次账号额度显示，无需手动点击 Refresh

**修复：XML 标签泄漏到对话历史**

Windsurf 代理使用 Anthropic API 内部 XML 格式（`<human>`、`</human>`、`</parameter>`、`</assistant>` 等），这些标签有时会以原始文本形式泄漏进对话，导致：

- 历史消息中出现裸露的 `<human>` / `</human>` 标签
- 模型看到这些标签后误以为是用户输入，产生错误的上下文理解

**修复方案**：在消息回放（`replay()`）和消息持久化（`flush_pending_message()`）阶段，过滤所有仅含 XML 标签和空白字符的文本块，使其不渲染也不存入历史。

---

#### 方式二：环境变量（CI / 无 UI 场景）

```sh
export WINDSURF_AUTH_TOKEN="YOUR_TOKEN"
```

#### 已支持模型

| 模型 | 备注 |
|------|------|
| claude-sonnet-4 | 推荐日常使用 |
| claude-haiku-4.5 | 快速响应 |
| claude-opus-4-7-high-thinking | ⚠️ 香港节点可能遇到 451 地理限制 |
| gpt-5 | GPT 系列 |
| gemini-2.5-pro / flash | 无地理限制，推荐替代 Claude |
| kimi-k2 | 无地理限制 |
| glm-4.7 | 无地理限制 |

> **关于 451 Geo-restricted 错误**：Windsurf 香港机房被 Anthropic 地理封锁，所有账号均受影响。  
> **解决方案**：切换到 `Gemini 2.5 Flash`、`Kimi K2` 或 `GLM-4.7` 等非 Anthropic 模型。

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
