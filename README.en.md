# Celadon

[![Release](https://img.shields.io/github/v/release/MageGojo/Celadon?color=7ea388)](https://github.com/MageGojo/Celadon/releases)
[![License](https://img.shields.io/badge/license-GPL--3.0%20%2F%20Apache--2.0-7a5f52)](./LICENSE-GPL)
[![Platform](https://img.shields.io/badge/platform-macOS-506956)](https://github.com/MageGojo/Celadon)
[![Based on Zed](https://img.shields.io/badge/based%20on-Zed-7a6f89)](https://github.com/zed-industries/zed)

> English · [中文](./README.md)

**Celadon** is a personal fork of the [Zed editor](https://github.com/zed-industries/zed) that preserves all upstream Zed functionality while adding a refined visual theme and several locale-aware feature enhancements.

The name *Celadon* (青瓷) refers to the muted sage-green glaze of Song-dynasty ceramics — also the dominant accent color of the bundled MorandiGarden theme.

---

## Features

### 1. MorandiGarden Preview Theme

Markdown (`.md`, `.markdown`) and HTML (`.html`, `.htm`) file previews are rendered using the **MorandiGarden** theme, with CSS adapted from [Kitsunee-CN/MorandiGarden](https://github.com/Kitsunee-CN/MorandiGarden) (originally a Typora theme).

The preview pane uses macOS `WKWebView` (WebKit) for browser-grade rendering:

- Soft off-white background (`#FBFDFB`) with warm-toned body text
- Six heading levels each carry a distinct Morandi color (warm brown → sage green → muted purple → slate blue → tan → warm grey)
- `h1` has a soft underline; `h2` has a celadon-green left bar
- Code blocks: rounded corners, celadon background
- Blockquotes: nested depth supported, each level a different shade
- Task-list checkboxes rendered as smooth circular toggles
- Tables: celadon-green headers with hover row highlight

To open a preview, press `Cmd+Shift+P` → **Preview: Open Preview**, or click the 👁 icon in the editor toolbar.

### 2. Windsurf AI Integration

Celadon ships with a built-in **Windsurf language-model provider** that connects to the local Windsurf proxy service. When the Windsurf IDE is running, Celadon automatically picks up authentication from:

```
~/Library/Application Support/Windsurf/User/globalStorage/
  sparkcore.xinghuo-windsurf/devin-session-cache.json
```

No manual API key setup is required. All models available in your Windsurf subscription (Claude, GPT, Gemini, DeepSeek, etc.) appear automatically in Celadon's AI panel.

Configuration (optional — all values have defaults):

```json
{
  "language_models": {
    "windsurf": {
      "api_url": "http://localhost:3003/v1"
    }
  }
}
```

You can also set the `WINDSURF_AUTH_TOKEN` environment variable for manual authentication.

### 3. Bilingual UI (English / 简体中文)

A built-in `i18n` module provides UI translation between English and Simplified Chinese.

```json
{
  "locale": "zh-CN"
}
```

Supported values: `"en"` (English, default), `"zh-CN"` (Simplified Chinese).

Coverage includes the macOS app menu, command palette, file menu, project-panel context menu, settings UI, agent panel, dock, and pane tabs — most major UI surfaces.

### 4. Workspace Background Image

Set a background image for the Celadon main window. When set, the editor background and gutter automatically drop to 85% opacity so the image shows through.

```json
{
  "experimental.theme_overrides": {
    "background.image_file": "/Users/you/Pictures/celadon-wall.png"
  }
}
```

PNG and JPEG are supported. The setting is also exposed in the visual Settings UI.

### 5. `.http` / `.rest` File Runner

Edit and execute HTTP requests directly inside Celadon — no need to switch to Postman or curl.

Open any `.http` or `.rest` file and a ▶️ run button appears automatically in the toolbar:

- Built-in [tree-sitter-http](https://github.com/rest-nvim/tree-sitter-http) syntax highlighting
- Parses the request block at the cursor position
- One-click execution; the response (status, headers, body) opens as a new buffer
- Response is auto-highlighted as HTTP, supports HTTP/1.1, HTTP/2, HTTP/3

Example `api.http`:

```http
### List users
GET https://api.example.com/users
Authorization: Bearer YOUR_TOKEN

### Create user
POST https://api.example.com/users
Content-Type: application/json

{
  "name": "Alice",
  "email": "alice@example.com"
}
```

Place the cursor inside any `### ...` block and click run to send that request.

---

## Building

### Prerequisites

- Rust (via [rustup](https://www.rust-lang.org/tools/install))
- macOS: Xcode and Xcode Command Line Tools
- `cmake` (`brew install cmake`)

### Dev build (run directly)

The first build requires downloading a pre-built `libwebrtc.a` (~300 MB). If automatic download fails due to a TLS interruption, fetch it manually:

```sh
# Manually download libwebrtc
curl -L -o /tmp/webrtc-mac-arm64-release.zip \
  "https://github.com/zed-industries/livekit-rust-sdks/releases/download/webrtc-0001d84-4/webrtc-mac-arm64-release.zip"
mkdir -p ~/webrtc-prebuilt
unzip /tmp/webrtc-mac-arm64-release.zip -d ~/webrtc-prebuilt

# Build and run
LK_CUSTOM_WEBRTC=~/webrtc-prebuilt/mac-arm64-release cargo run
```

For Intel Macs, replace `arm64` with `x64` in the commands above.

### macOS `.app` bundle (with icon)

Install the customized [`cargo-bundle`](https://github.com/zed-industries/cargo-bundle):

```sh
cargo install cargo-bundle \
  --git https://github.com/zed-industries/cargo-bundle.git \
  --branch zed-deploy
```

Build the `.app` bundle:

```sh
LK_CUSTOM_WEBRTC=~/webrtc-prebuilt/mac-arm64-release \
  script/bundle-mac -d -o
```

`-d` uses a debug build (faster), `-o` opens the result automatically.

> **Custom icon**: Drop your own PNG (1024×1024 recommended) over `crates/zed/resources/app-icon-dev.png` and `crates/zed/resources/app-icon-dev@2x.png`.

---

## Acknowledgements & Copyright

Celadon is a derivative of [Zed](https://github.com/zed-industries/zed); **all copyright and licensing is inherited from the upstream project**. Zed is developed by **Zed Industries, Inc.** This repository claims no trademark or copyright over Zed itself.

- Upstream licenses: [GPL-3.0-or-later](./LICENSE-GPL) and [Apache-2.0](./LICENSE-APACHE) (per-crate `LICENSE-*` files preserved as-is)
- Third-party dependency licenses: [`script/licenses/zed-licenses.toml`](./script/licenses/zed-licenses.toml)
- MorandiGarden theme: [Kitsunee-CN](https://github.com/Kitsunee-CN/MorandiGarden)
- tree-sitter-http: [rest-nvim/tree-sitter-http](https://github.com/rest-nvim/tree-sitter-http)

This project is for personal use and experimentation only; no commercial distribution. All source code remains bound by the original open-source licenses, and every upstream copyright notice, `LICENSE-*` file, and per-crate license header is preserved unchanged.

To contribute upstream, please head to [zed-industries/zed](https://github.com/zed-industries/zed) and the official [CONTRIBUTING.md](./CONTRIBUTING.md).
