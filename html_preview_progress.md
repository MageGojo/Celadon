# HTML 内部渲染进度文档

## 可行方案确认

### 核心技术路径

1. **GPUI `Window` 实现了 `HasWindowHandle` trait**（`raw-window-handle 0.6.2`）
   - 文件：`crates/gpui/src/window.rs:5671`
   - macOS 平台实现：`crates/gpui_macos/src/window.rs`

2. **`wry` crate 支持 `build_as_child(&impl HasWindowHandle)`**
   - wry 0.44 兼容 raw-window-handle 0.6
   - 在 macOS 上将 WKWebView 嵌入现有 NSWindow 的 contentView
   - Zed 已用 `objc2-app-kit = "0.3"`，与 wry 0.44 同一 objc2 体系，无冲突

3. **GPUI `canvas` 元素**（`crates/gpui/src/elements/canvas.rs`）
   - `prepaint` 回调在 layout 后触发，携带像素级 `Bounds<Pixels>`
   - 闭包要求 `'static + FnOnce`，**无 `Send` 要求**
   - 可用 `Rc<RefCell<Option<wry::WebView>>>` 在 entity 和 canvas 之间共享

### 架构图

```
GPUI Window（Metal GPU 渲染）
├── Toolbar / Tab / Editor  →  GPUI 原生渲染
└── 预览 Pane
    └── HtmlPreviewView（Entity）
        └── canvas element
            ├── prepaint: 知道 Bounds → 创建/更新 wry WebView 位置
            └── paint: 空（WebView 自己绘制 HTML）
```

---

## 需要修改/创建的文件

### 新建文件

| 文件 | 说明 |
|------|------|
| `crates/html_preview/Cargo.toml` | 新 crate，依赖 wry（macOS-only） |
| `crates/html_preview/src/html_preview.rs` | crate root，actions + init |
| `crates/html_preview/src/html_preview_view.rs` | HtmlPreviewView entity，Item 实现，wry 集成 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `Cargo.toml`（workspace） | 添加 `wry = "0.44"`；添加 `html_preview` 到 members |
| `crates/zed_actions/src/lib.rs` | ✅ 已添加 `preview::html::{ OpenPreview, OpenPreviewToTheSide }` |
| `crates/zed/Cargo.toml` | 添加 `html_preview.workspace = true` |
| `crates/zed/src/zed/quick_action_bar/preview.rs` | 添加 `Html` 到 `PreviewType`，检测 `.html`/`.htm` 文件 |
| `crates/zed/src/main.rs` 或 app init | 调用 `html_preview::init(cx)` |

---

## 关键实现细节

### 坐标转换

GPUI `Pixels` 是逻辑像素，wry 要求物理像素：
```rust
let scale = window.scale_factor();
// macOS wry 使用 top-left 原点（与 GPUI 相同）
position: PhysicalPosition::new(
    (bounds.origin.x.0 * scale) as i32,
    (bounds.origin.y.0 * scale) as i32,
)
```

### WebView 生命周期

- `HtmlPreviewView` entity 持有 `Rc<RefCell<Option<wry::WebView>>>`
- 第一次 `prepaint` 时调用 `wry::WebViewBuilder::new().build_as_child(window)` 创建
- 每帧 `prepaint` 调用 `webview.set_bounds(...)` 更新位置
- entity drop 时 `wry::WebView` 自动 drop，WebView 从 NSWindow 中移除

### buffer 变化实时更新

```rust
cx.subscribe(&editor, |this, _, event, cx| {
    // 过滤 buffer 内容变化事件
    this.html = get_buffer_text(&this.editor, cx);
    if let Some(ref wv) = *this.webview.borrow() {
        let _ = wv.load_html(&this.html, None);
    }
});
```

### Item trait 最小实现（参考 svg_preview 模式）

```rust
impl Item for HtmlPreviewView {
    type Event = ();
    fn tab_icon(...) -> Option<Icon> { Some(Icon::new(IconName::FileHtml)) }
    fn tab_content_text(...) -> SharedString { format!("Preview {}", filename).into() }
    fn can_save(...) -> bool { false }
    fn is_dirty(...) -> bool { false }
    fn act_as_type(&self, type_id, self_handle, _) -> Option<AnyEntity> {
        // 允许 workspace 通过 editor type 找到关联的 editor
    }
}
```

---

## 当前进度

- [x] 确认技术可行性（GPUI HasWindowHandle + WKWebView via objc/cocoa）
- [x] 添加 `zed_actions::preview::html` actions
- [x] 创建 `crates/html_preview` crate（Cargo.toml + src）
- [x] 实现 HtmlPreviewView（canvas + WKWebView macOS overlay）
- [x] 更新 preview.rs 支持 .html/.htm 文件
- [x] 注册 init（zed/src/main.rs）
- [x] 编译测试通过（`cargo build -p zed` 零错误）

## 实际技术路径（与预案差异）

- **放弃 wry**：改用 `cocoa` + `objc` crate（workspace 已有）直接通过 `msg_send!` 创建 WKWebView，避免 objc2 版本冲突风险。
- **`#[link(name = "WebKit", kind = "framework")]`**：在 `html_preview_view.rs` 中直接声明 WebKit 框架链接。
- **坐标转换**：`NSView.y = viewport_height - gpui_y - view_height`（NSView bottom-left origin 转 GPUI top-left origin）。

---

## 已知风险

1. **objc2 版本冲突**：Zed 固定 `objc2-foundation = "=0.3.2"`，wry 0.44 可能要求不同补丁版本 → 如有冲突改用 `wry = "0.43"`
2. **坐标系**：macOS wry `set_bounds` Y 轴方向未完全确认（top-left vs bottom-left）→ 编译后测试确认，若反了加翻转
3. **`wry::WebView` 是 `!Send`**：canvas 闭包无 `Send` 要求，用 `Rc<RefCell<>>` 安全
