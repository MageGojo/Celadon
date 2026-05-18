# Celadon

**Celadon** is a personalized fork of [Zed](https://github.com/zed-industries/zed) — a high-performance, multiplayer code editor — enhanced with a refined visual theme and Windsurf AI integration.

The name *Celadon* (青瓷) refers to the muted sage-green glaze characteristic of Song-dynasty ceramics, which is also the defining color of the MorandiGarden theme used throughout this build.

---

## What's Different from Upstream Zed

### MorandiGarden Preview Theme

Markdown (`.md`, `.markdown`) and HTML (`.html`, `.htm`) file previews are rendered using the **MorandiGarden** theme — a low-saturation, garden-inspired palette adapted from the [Typora MorandiGarden theme](https://github.com/Kitsunee-CN/MorandiGarden).

The preview pane uses macOS `WKWebView` (WebKit) for pixel-accurate rendering. Key visual characteristics:

- Light background (`#FBFDFB`) with warm-toned body text
- Six heading levels each carry a distinct Morandi color (warm brown → sage green → muted purple → slate → tan → warm grey)
- `h1` has a soft bottom border; `h2` has a sage left-bar accent
- Code blocks: rounded corners, celadon background
- Blockquotes: left-bordered, italic, nested depth supported
- Task-list checkboxes rendered as smooth circular toggles
- Tables: sage-green headers with hover highlight

To open a preview, press `Cmd+Shift+P` → **Preview: Open Preview** (or click the eye icon in the editor toolbar).

### Windsurf AI Integration

Celadon includes a built-in **Windsurf AI provider** that connects to the local Windsurf proxy service. When the Windsurf IDE is running, Celadon automatically picks up authentication from:

```
~/Library/Application Support/Windsurf/User/globalStorage/
  sparkcore.xinghuo-windsurf/devin-session-cache.json
```

No manual token setup is required. All models available in your Windsurf subscription (Claude, GPT, Gemini, DeepSeek, etc.) appear automatically in Celadon's AI panel.

**Configuration** (optional — all values have defaults):

```json
{
  "language_models": {
    "windsurf": {
      "api_url": "http://localhost:3003/v1"
    }
  }
}
```

You can also set `WINDSURF_AUTH_TOKEN` as an environment variable if you prefer manual authentication.

---

## Building

### Prerequisites

- Rust (via [rustup](https://www.rust-lang.org/tools/install))
- Xcode and Xcode Command Line Tools (macOS)
- `cmake` (`brew install cmake`)

### Dev build (run directly)

The first build requires downloading a pre-built `libwebrtc.a` (~300 MB). If the automatic download fails due to a TLS issue, use the manual path:

```sh
# Download libwebrtc manually
curl -L -o /tmp/webrtc-mac-arm64-release.zip \
  "https://github.com/zed-industries/livekit-rust-sdks/releases/download/webrtc-0001d84-4/webrtc-mac-arm64-release.zip"
mkdir -p ~/webrtc-prebuilt
unzip /tmp/webrtc-mac-arm64-release.zip -d ~/webrtc-prebuilt

# Build and run
LK_CUSTOM_WEBRTC=~/webrtc-prebuilt/mac-arm64-release cargo run
```

For Intel Macs, replace `arm64` with `x64` in the above commands.

### macOS app bundle (with icon)

Install [`cargo-bundle`](https://github.com/zed-industries/cargo-bundle):

```sh
cargo install cargo-bundle \
  --git https://github.com/zed-industries/cargo-bundle.git \
  --branch zed-deploy
```

Then build the `.app` bundle:

```sh
LK_CUSTOM_WEBRTC=~/webrtc-prebuilt/mac-arm64-release \
  script/bundle-mac -d -o
```

The `-d` flag uses a debug build (faster), `-o` opens the result automatically.

> **Icon**: Place your custom `Celadon.icns` at `crates/zed/resources/Celadon.icns` and the bundle script will pick it up automatically.

---

## Upstream

Celadon is based on [Zed](https://github.com/zed-industries/zed) by Zed Industries, Inc.
Original license: GPL-3.0-or-later / Apache-2.0 (see individual crate headers).

This fork is for personal use and experimentation. All upstream features of Zed are preserved.
