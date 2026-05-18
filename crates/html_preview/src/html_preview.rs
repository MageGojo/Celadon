use gpui::{App, actions};
use workspace::Workspace;

pub mod html_preview_view;

pub use zed_actions::preview::html::{OpenPreview, OpenPreviewToTheSide};

actions!(
    html,
    [
        /// Opens a following HTML preview that syncs with the editor.
        OpenFollowingPreview,
        /// Opens the HTML file in the default browser.
        OpenInBrowser,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };
        crate::html_preview_view::HtmlPreviewView::register(workspace, window, cx);
    })
    .detach();
}
