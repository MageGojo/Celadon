use file_icons::FileIcons;
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement,
    Render, Styled, Subscription, WeakEntity, Window, canvas, div,
};
use language::{Buffer, BufferEvent};
use multi_buffer::MultiBuffer;
use pulldown_cmark::{Options, Parser, html as cmark_html};
use ui::prelude::*;
use workspace::item::Item;
use workspace::{Pane, Workspace};

use crate::{OpenFollowingPreview, OpenInBrowser, OpenPreview, OpenPreviewToTheSide};

#[cfg(target_os = "macos")]
use std::cell::RefCell;
#[cfg(target_os = "macos")]
use std::rc::Rc;

pub struct HtmlPreviewView {
    focus_handle: FocusHandle,
    buffer: Option<Entity<Buffer>>,
    is_markdown: bool,
    html_content: String,
    #[cfg(target_os = "macos")]
    webview: Rc<RefCell<Option<MacWebView>>>,
    _buffer_subscription: Option<Subscription>,
    _workspace_subscription: Option<Subscription>,
    _focus_subscriptions: Vec<Subscription>,
}

#[cfg(target_os = "macos")]
struct MacWebView {
    view: cocoa::base::id,
    temp_path: std::path::PathBuf,
}

#[cfg(target_os = "macos")]
impl MacWebView {
    fn new(
        parent: cocoa::base::id,
        frame: cocoa::foundation::NSRect,
        html: &str,
    ) -> Option<Self> {
        use cocoa::base::nil;
        use objc::{class, msg_send, sel, sel_impl};
        use objc::runtime::YES;

        let temp_path = Self::make_temp_path();
        std::fs::write(&temp_path, html).ok()?;

        unsafe {
            let config: cocoa::base::id = msg_send![class!(WKWebViewConfiguration), new];
            let view: cocoa::base::id = msg_send![class!(WKWebView), alloc];
            let view: cocoa::base::id =
                msg_send![view, initWithFrame:frame configuration:config];
            if view == nil {
                return None;
            }
            let () = msg_send![parent, addSubview: view];
            let () = msg_send![view, setWantsLayer: YES];
            let layer: cocoa::base::id = msg_send![view, layer];
            if layer != nil {
                let () = msg_send![layer, setMasksToBounds: YES];
            }
            let wv = MacWebView { view, temp_path };
            wv.load_from_temp_file();
            let window: cocoa::base::id = msg_send![parent, window];
            if window != nil {
                let () = msg_send![window, makeFirstResponder: parent];
            }
            Some(wv)
        }
    }

    fn make_temp_path() -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("zed_html_preview_{}.html", ts))
    }

    fn load_from_temp_file(&self) {
        use cocoa::foundation::NSString;
        use cocoa::base::nil;
        use objc::{class, msg_send, sel, sel_impl};
        unsafe {
            if let Some(path_str) = self.temp_path.to_str() {
                let ns_path = NSString::alloc(nil).init_str(path_str);
                let file_url: cocoa::base::id =
                    msg_send![class!(NSURL), fileURLWithPath: ns_path];
                let () = msg_send![self.view, loadFileURL: file_url
                                         allowingReadAccessToURL: file_url];
            }
        }
    }

    fn update_html(&self, html: &str) {
        std::fs::write(&self.temp_path, html).ok();
        self.load_from_temp_file();
    }

    fn set_frame(&self, frame: cocoa::foundation::NSRect) {
        use objc::{msg_send, sel, sel_impl};
        unsafe {
            let () = msg_send![self.view, setFrame: frame];
        }
    }

    fn set_hidden(&self, hidden: bool) {
        use cocoa::base::nil;
        use objc::{msg_send, sel, sel_impl};
        use objc::runtime::BOOL;
        unsafe {
            if hidden {
                let window: cocoa::base::id = msg_send![self.view, window];
                if window != nil {
                    let parent: cocoa::base::id = msg_send![self.view, superview];
                    if parent != nil {
                        let () = msg_send![window, makeFirstResponder: parent];
                    }
                }
            }
            let () = msg_send![self.view, setHidden: hidden as BOOL];
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacWebView {
    fn drop(&mut self) {
        use objc::{msg_send, sel, sel_impl};
        unsafe {
            let () = msg_send![self.view, removeFromSuperview];
        }
        std::fs::remove_file(&self.temp_path).ok();
    }
}

#[cfg(target_os = "macos")]
#[link(name = "WebKit", kind = "framework")]
unsafe extern "C" {}

fn md_to_html(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    let parser = Parser::new_ext(md, opts);
    let mut out = String::new();
    cmark_html::push_html(&mut out, parser);
    out
}

fn html_template(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>
/* MorandiGarden — adapted from https://github.com/Kitsunee-CN/MorandiGarden */
:root {{
  --bg-color: #FBFDFB;
  --text-color: #403C3C;
  --h1-color: #7A5F52;
  --h2-color: #506956;
  --h3-color: #7A6F89;
  --h4-color: #6F7F88;
  --h5-color: #8A7A6F;
  --h6-color: #7D7A71;
  --link-color: #506956;
  --link-hover-color: #405946;
  --code-bg-color: #F0F5F3;
  --code-text-color: #403C3C;
  --blockquote-bg-color: #EEF4EF;
  --blockquote-border-color: #7A5F52;
  --table-border-color: #7ea388;
  --th-bg-color: #EAF0EB;
  --h1-size: 2.2rem;
  --h2-size: 1.8rem;
  --h3-size: 1.5rem;
  --h4-size: 1.3rem;
  --h5-size: 1.1rem;
  --h6-size: 1rem;
  --line-height: 1.8;
  --font-size-base: 16px;
}}
* {{ box-sizing: border-box; }}
html {{ font-size: var(--font-size-base); }}
body {{
  background-color: var(--bg-color);
  color: var(--text-color);
  font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB",
               "Segoe UI", Helvetica, Arial, sans-serif;
  line-height: var(--line-height);
  max-width: 860px;
  margin: 0 auto;
  padding: 2rem 1.5rem 4rem;
}}
h1, h2, h3, h4, h5, h6 {{
  margin: 1.8rem 0 1rem;
  font-weight: 700;
  line-height: 1.4;
  letter-spacing: 0.5px;
  border: none;
  padding: 0;
}}
h1 {{ font-size: var(--h1-size); color: var(--h1-color); border-bottom: 2px solid #DDE6DF; padding-bottom: 0.6rem; }}
h2 {{ font-size: var(--h2-size); color: var(--h2-color); border-left: 4px solid var(--h2-color); padding-left: 0.8rem; font-weight: 800; }}
h3 {{ font-size: var(--h3-size); color: var(--h3-color); padding-left: 0.4rem; }}
h4 {{ font-size: var(--h4-size); color: var(--h4-color); }}
h5 {{ font-size: var(--h5-size); color: var(--h5-color); font-style: italic; }}
h6 {{ font-size: var(--h6-size); color: var(--h6-color); font-weight: 400; }}
p {{ margin: 0.8em 0; }}
a {{
  color: var(--link-color);
  text-decoration: none;
  border-bottom: 1px solid rgba(80, 105, 86, 0.3);
  transition: all 0.25s ease;
}}
a:hover {{ color: var(--link-hover-color); border-bottom-color: var(--link-hover-color); }}
strong {{ color: var(--h1-color); }}
em {{ font-style: italic; }}
del {{ color: #999; text-decoration: line-through; }}
code {{
  font-family: "JetBrains Mono", "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.875em;
  color: var(--h3-color);
  background: var(--code-bg-color);
  padding: 3px 6px;
  border-radius: 5px;
}}
pre {{
  background: var(--code-bg-color);
  border-radius: 10px;
  padding: 1.2rem 1rem;
  overflow-x: auto;
  margin: 1em 0;
  line-height: 1.6;
}}
pre code {{
  background: none;
  padding: 0;
  border-radius: 0;
  font-size: 0.9em;
  color: var(--code-text-color);
}}
blockquote {{
  margin: 1.5rem 0;
  padding: 1rem 1.2rem;
  background: var(--blockquote-bg-color);
  border-left: 4px solid var(--blockquote-border-color);
  border-radius: 0 10px 10px 0;
  font-style: italic;
}}
blockquote p {{ margin: 0.3em 0; }}
blockquote blockquote {{
  border-left-color: var(--h3-color);
  background: rgba(238, 244, 239, 0.6);
  margin: 0.8rem 0;
}}
blockquote blockquote blockquote {{
  border-left-color: var(--h4-color);
  background: rgba(238, 244, 239, 0.4);
}}
ul, ol {{
  margin: 1rem 0 1rem 1.6rem;
  padding-left: 0;
}}
ul {{ list-style-type: disc; }}
ul ul {{ list-style-type: circle; margin-left: 1.2rem; }}
ul ul ul {{ list-style-type: square; margin-left: 1.2rem; }}
ol {{ list-style-type: decimal; }}
ol ol {{ margin-left: 1.2rem; }}
li {{
  margin: 0.5rem 0;
  line-height: 1.7;
}}
li input[type="checkbox"] {{
  -webkit-appearance: none;
  appearance: none;
  width: 1.1rem;
  height: 1.1rem;
  border-radius: 50%;
  border: 2px solid #8F9FA8;
  background: transparent;
  vertical-align: middle;
  margin-right: 0.5rem;
  position: relative;
  top: -1px;
  transition: background 0.2s ease-in-out, border-color 0.2s ease-in-out;
}}
li input[type="checkbox"]:checked {{
  background: #708976;
  border-color: #708976;
}}
table {{
  border-collapse: collapse;
  width: 100%;
  margin: 2rem 0;
  overflow: hidden;
  font-size: 0.95em;
}}
th {{
  background: var(--th-bg-color);
  padding: 0.9rem 1rem;
  font-weight: 700;
  color: var(--h2-color);
  border: 1px solid var(--table-border-color);
  text-align: left;
}}
td {{
  padding: 0.8rem 1rem;
  border: 1px solid var(--table-border-color);
  color: var(--text-color);
  vertical-align: top;
}}
tr:hover {{ background: rgba(238, 244, 239, 0.5); }}
hr {{
  border: none;
  height: 2px;
  background: #ccd8cf;
  margin: 2.5rem 0;
}}
img {{ max-width: 100%; height: auto; border-radius: 6px; display: block; margin: 0.5em 0; }}
</style></head>
<body>
{body}
</body></html>"#,
        body = body
    )
}

fn buffer_html(buffer: &Entity<Buffer>, cx: &App) -> String {
    buffer.read(cx).text()
}

fn compute_display_html(raw: &str, is_markdown: bool) -> String {
    if is_markdown {
        html_template(&md_to_html(raw))
    } else {
        raw.to_owned()
    }
}

impl HtmlPreviewView {
    pub fn new(
        active_buffer: Entity<MultiBuffer>,
        _workspace_handle: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let buffer = active_buffer.read_with(cx, |b, _| b.as_singleton());

            let is_markdown = buffer
                .as_ref()
                .and_then(|b| b.read(cx).file())
                .is_some_and(|f| {
                    let name = f.file_name(cx);
                    let ext = std::path::Path::new(name)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
                });

            let raw = buffer
                .as_ref()
                .map(|b| buffer_html(b, cx))
                .unwrap_or_default();

            let html_content = compute_display_html(&raw, is_markdown);

            let subscription = buffer
                .as_ref()
                .map(|b| Self::create_buffer_subscription(b, window, cx));

            let focus_handle = cx.focus_handle();

            let focus_out = cx.on_focus_out(
                &focus_handle,
                window,
                |this, _event, window, _cx| {
                    #[cfg(target_os = "macos")]
                    {
                        use cocoa::base::nil;
                        use objc::{msg_send, sel, sel_impl};
                        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                        if let Some(wv) = this.webview.borrow().as_ref() {
                            let in_wv = (|| -> bool {
                                let Ok(h) = window.window_handle() else { return false };
                                let nsv = match h.as_raw() {
                                    RawWindowHandle::AppKit(a) => a.ns_view.as_ptr() as cocoa::base::id,
                                    _ => return false,
                                };
                                unsafe {
                                    let nsw: cocoa::base::id = msg_send![nsv, window];
                                    if nsw == nil { return false; }
                                    let fr: cocoa::base::id = msg_send![nsw, firstResponder];
                                    if fr == nil { return false; }
                                    let is_desc: bool = msg_send![fr, isDescendantOf: wv.view];
                                    is_desc
                                }
                            })();
                            if !in_wv {
                                wv.set_hidden(true);
                            }
                        }
                    }
                },
            );

            let focus_in = cx.on_focus_in(
                &focus_handle,
                window,
                |this, _window, cx| {
                    #[cfg(target_os = "macos")]
                    if this.webview.borrow().is_some() {
                        cx.notify();
                    }
                },
            );

            Self {
                focus_handle,
                buffer,
                is_markdown,
                html_content,
                #[cfg(target_os = "macos")]
                webview: Rc::new(RefCell::new(None)),
                _buffer_subscription: subscription,
                _workspace_subscription: None,
                _focus_subscriptions: vec![focus_out, focus_in],
            }
        })
    }

    fn create_buffer_subscription(
        buffer: &Entity<Buffer>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(
            buffer,
            window,
            move |this, buffer, event: &BufferEvent, _window, cx| match event {
                BufferEvent::Edited { .. } | BufferEvent::Saved => {
                    let raw = buffer_html(buffer, cx);
                    this.html_content = compute_display_html(&raw, this.is_markdown);
                    #[cfg(target_os = "macos")]
                    if let Some(wv) = this.webview.borrow().as_ref() {
                        wv.update_html(&this.html_content);
                    }
                    cx.notify();
                }
                _ => {}
            },
        )
    }

    fn find_existing_preview_item_idx(
        pane: &Pane,
        buffer: &Entity<MultiBuffer>,
        cx: &App,
    ) -> Option<usize> {
        let buffer_id = buffer.read(cx).as_singleton()?.entity_id();
        pane.items_of_type::<HtmlPreviewView>()
            .find(|view| {
                view.read(cx)
                    .buffer
                    .as_ref()
                    .is_some_and(|b| b.entity_id() == buffer_id)
            })
            .and_then(|view| pane.index_for_item(&view))
    }

    pub fn resolve_active_item_as_html_buffer(
        workspace: &Workspace,
        cx: &mut Context<Workspace>,
    ) -> Option<Entity<MultiBuffer>> {
        workspace
            .active_item(cx)?
            .act_as::<MultiBuffer>(cx)
            .filter(|buffer| Self::is_html_file(buffer, cx))
    }

    pub fn is_html_file(buffer: &Entity<MultiBuffer>, cx: &App) -> bool {
        buffer
            .read(cx)
            .as_singleton()
            .and_then(|b| b.read(cx).file())
            .is_some_and(|file| {
                let name = file.file_name(cx);
                std::path::Path::new(name)
                    .extension()
                    .is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("html")
                            || ext.eq_ignore_ascii_case("htm")
                            || ext.eq_ignore_ascii_case("md")
                            || ext.eq_ignore_ascii_case("markdown")
                    })
            })
    }

    pub fn register(workspace: &mut Workspace, _window: &mut Window, _cx: &mut Context<Workspace>) {
        workspace.register_action(move |workspace, _: &OpenPreview, window, cx| {
            if let Some(buffer) = Self::resolve_active_item_as_html_buffer(workspace, cx) {
                let view = Self::new(buffer.clone(), workspace.weak_handle(), window, cx);
                workspace.active_pane().update(cx, |pane, cx| {
                    if let Some(idx) = Self::find_existing_preview_item_idx(pane, &buffer, cx) {
                        pane.activate_item(idx, true, true, window, cx);
                    } else {
                        pane.add_item(Box::new(view), true, true, None, window, cx);
                    }
                });
                cx.notify();
            }
        });

        workspace.register_action(move |workspace, _: &OpenPreviewToTheSide, window, cx| {
            if let Some(buffer) = Self::resolve_active_item_as_html_buffer(workspace, cx) {
                let view = Self::new(buffer.clone(), workspace.weak_handle(), window, cx);
                let pane = workspace
                    .find_pane_in_direction(workspace::SplitDirection::Right, cx)
                    .unwrap_or_else(|| {
                        workspace.split_pane(
                            workspace.active_pane().clone(),
                            workspace::SplitDirection::Right,
                            window,
                            cx,
                        )
                    });
                pane.update(cx, |pane, cx| {
                    if let Some(idx) = Self::find_existing_preview_item_idx(pane, &buffer, cx) {
                        pane.activate_item(idx, true, true, window, cx);
                    } else {
                        pane.add_item(Box::new(view), false, false, None, window, cx);
                    }
                });
                cx.notify();
            }
        });

        workspace.register_action(move |workspace, _: &OpenFollowingPreview, window, cx| {
            if let Some(buffer) = Self::resolve_active_item_as_html_buffer(workspace, cx) {
                let view = Self::new(buffer, workspace.weak_handle(), window, cx);
                workspace.active_pane().update(cx, |pane, cx| {
                    pane.add_item(Box::new(view), true, true, None, window, cx);
                });
                cx.notify();
            }
        });

        workspace.register_action(move |workspace, _: &OpenInBrowser, _window, cx| {
            let url =
                // Case 1: active item is the HtmlPreviewView itself
                workspace
                    .active_item(cx)
                    .and_then(|item| item.act_as::<HtmlPreviewView>(cx))
                    .and_then(|view| {
                        let b = view.read(cx).buffer.as_ref()?.clone();
                        let buf = b.read(cx);
                        let path = buf.file()?.full_path(cx);
                        Some(format!("file://{}", path.display()))
                    })
                    // Case 2: active item is the HTML/MD source editor
                    .or_else(|| {
                        Self::resolve_active_item_as_html_buffer(workspace, cx)
                            .and_then(|mb| {
                                let b = mb.read(cx).as_singleton()?;
                                let buf = b.read(cx);
                                let path = buf.file()?.full_path(cx);
                                Some(format!("file://{}", path.display()))
                            })
                    });
            if let Some(url) = url {
                cx.open_url(&url);
            }
        });
    }
}

impl Render for HtmlPreviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let html = self.html_content.clone();

        #[cfg(target_os = "macos")]
        {
            let webview_rc = self.webview.clone();
            div()
                .id("HtmlPreview")
                .key_context("HtmlPreview")
                .track_focus(&self.focus_handle(cx))
                .size_full()
                .bg(cx.theme().colors().editor_background)
                .child(
                    canvas(
                        move |bounds, window, _cx| {
                            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                            use cocoa::foundation::{NSPoint, NSRect, NSSize};

                            let Ok(handle) = window.window_handle() else {
                                return;
                            };
                            let parent_ns_view = match handle.as_raw() {
                                RawWindowHandle::AppKit(h) => {
                                    h.ns_view.as_ptr() as cocoa::base::id
                                }
                                _ => return,
                            };

                            let vp = window.viewport_size();
                            let parent_h = vp.height.as_f32() as f64;

                            let x = bounds.origin.x.as_f32() as f64;
                            let gpui_y = bounds.origin.y.as_f32() as f64;
                            let w = bounds.size.width.as_f32() as f64;
                            let h = bounds.size.height.as_f32() as f64;
                            let y = parent_h - gpui_y - h;

                            let frame = NSRect::new(
                                NSPoint::new(x, y),
                                NSSize::new(w, h),
                            );

                            let mut state = webview_rc.borrow_mut();
                            if let Some(wv) = state.as_ref() {
                                wv.set_hidden(false);
                                wv.set_frame(frame);
                            } else if let Some(wv) =
                                MacWebView::new(parent_ns_view, frame, &html)
                            {
                                *state = Some(wv);
                            }
                        },
                        |_bounds, _state, _window, _cx| {},
                    )
                    .size_full(),
                )
        }

        #[cfg(not(target_os = "macos"))]
        div()
            .id("HtmlPreview")
            .key_context("HtmlPreview")
            .track_focus(&self.focus_handle(cx))
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .flex()
            .justify_center()
            .items_center()
            .child(div().p_4().child("HTML preview is only available on macOS."))
    }
}

impl Focusable for HtmlPreviewView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for HtmlPreviewView {}

impl Item for HtmlPreviewView {
    type Event = ();

    fn tab_icon(&self, _window: &Window, cx: &App) -> Option<Icon> {
        self.buffer
            .as_ref()
            .and_then(|b| b.read(cx).file())
            .and_then(|file| FileIcons::get_icon(file.path().as_std_path(), cx))
            .map(Icon::from_path)
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.buffer
            .as_ref()
            .and_then(|b| b.read(cx).file())
            .map(|file| format!("Preview {}", file.file_name(cx)).into())
            .unwrap_or_else(|| "HTML Preview".into())
    }

    fn telemetry_event_text(&self) -> Option<&'static str> {
        Some("html preview: open")
    }

    fn deactivated(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        if let Some(wv) = self.webview.borrow().as_ref() {
            wv.set_hidden(true);
        }
    }

    fn tab_extra_context_menu_actions(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Vec<(SharedString, Box<dyn gpui::Action>)> {
        vec![("Open in Browser".into(), Box::new(OpenInBrowser))]
    }

    fn to_item_events(_event: &Self::Event, _f: &mut dyn FnMut(workspace::item::ItemEvent)) {}
}
