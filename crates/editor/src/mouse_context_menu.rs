use crate::{
    Copy, CopyAndTrim, CopyPermalinkToLine, Cut, DisplayPoint, DisplaySnapshot, Editor,
    EvaluateSelectedText, FindAllReferences, GoToDeclaration, GoToDefinition, GoToImplementation,
    GoToTypeDefinition, Paste, Rename, RevealInFileManager, RunToCursor, SelectMode,
    SelectionEffects, SelectionExt, ToDisplayPoint, ToggleCodeActions,
    actions::{Format, FormatSelections},
    selections_collection::SelectionsCollection,
};
use gpui::prelude::FluentBuilder;
use gpui::{Context, DismissEvent, Entity, Focusable as _, Pixels, Point, Subscription, Window};
use i18n::t;
use project::DisableAiSettings;
use settings::Settings as _;
use std::ops::Range;
use text::PointUtf16;
use workspace::{OpenInTerminal, RunFilePathInTerminal, RunHttpRequest, WorkspaceSettings};
use zed_actions::agent::AddSelectionToThread;
use zed_actions::preview::{
    markdown::OpenPreview as OpenMarkdownPreview, svg::OpenPreview as OpenSvgPreview,
};

#[derive(Debug)]
pub enum MenuPosition {
    /// When the editor is scrolled, the context menu stays on the exact
    /// same position on the screen, never disappearing.
    PinnedToScreen(Point<Pixels>),
    /// When the editor is scrolled, the context menu follows the position it is associated with.
    /// Disappears when the position is no longer visible.
    PinnedToEditor {
        source: multi_buffer::Anchor,
        offset: Point<Pixels>,
    },
}

pub struct MouseContextMenu {
    pub(crate) position: MenuPosition,
    pub(crate) context_menu: Entity<ui::ContextMenu>,
    _dismiss_subscription: Subscription,
    _cursor_move_subscription: Subscription,
}

impl std::fmt::Debug for MouseContextMenu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MouseContextMenu")
            .field("position", &self.position)
            .field("context_menu", &self.context_menu)
            .finish()
    }
}

impl MouseContextMenu {
    pub(crate) fn pinned_to_editor(
        editor: &mut Editor,
        source: multi_buffer::Anchor,
        position: Point<Pixels>,
        context_menu: Entity<ui::ContextMenu>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<Self> {
        let editor_snapshot = editor.snapshot(window, cx);
        let content_origin = editor.last_bounds?.origin
            + Point {
                x: editor.gutter_dimensions.width,
                y: Pixels::ZERO,
            };
        let source_position = editor.to_pixel_point(source, &editor_snapshot, window, cx)?;
        let menu_position = MenuPosition::PinnedToEditor {
            source,
            offset: position - (source_position + content_origin),
        };
        Some(MouseContextMenu::new(
            editor,
            menu_position,
            context_menu,
            window,
            cx,
        ))
    }

    pub(crate) fn new(
        editor: &Editor,
        position: MenuPosition,
        context_menu: Entity<ui::ContextMenu>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Self {
        let context_menu_focus = context_menu.focus_handle(cx);

        // Since `ContextMenu` is rendered in a deferred fashion its focus
        // handle is not linked to the Editor's until after the deferred draw
        // callback runs.
        // We need to wait for that to happen before focusing it, so that
        // calling `contains_focused` on the editor's focus handle returns
        // `true` when the `ContextMenu` is focused.
        let focus_handle = context_menu_focus.clone();
        cx.on_next_frame(window, move |_, window, cx| {
            cx.on_next_frame(window, move |_, window, cx| {
                window.focus(&focus_handle, cx);
            });
        });

        let _dismiss_subscription = cx.subscribe_in(&context_menu, window, {
            let context_menu_focus = context_menu_focus.clone();
            move |editor, _, _event: &DismissEvent, window, cx| {
                editor.mouse_context_menu.take();
                if context_menu_focus.contains_focused(window, cx) {
                    window.focus(&editor.focus_handle(cx), cx);
                }
            }
        });

        let selection_init = editor.selections.newest_anchor().clone();

        let _cursor_move_subscription = cx.subscribe_in(
            &cx.entity(),
            window,
            move |editor, _, event: &crate::EditorEvent, window, cx| {
                let crate::EditorEvent::SelectionsChanged { local: true } = event else {
                    return;
                };
                let display_snapshot = &editor
                    .display_map
                    .update(cx, |display_map, cx| display_map.snapshot(cx));
                let selection_init_range = selection_init.display_range(display_snapshot);
                let selection_now_range = editor
                    .selections
                    .newest_anchor()
                    .display_range(display_snapshot);
                if selection_now_range == selection_init_range {
                    return;
                }
                editor.mouse_context_menu.take();
                if context_menu_focus.contains_focused(window, cx) {
                    window.focus(&editor.focus_handle(cx), cx);
                }
            },
        );

        Self {
            position,
            context_menu,
            _dismiss_subscription,
            _cursor_move_subscription,
        }
    }
}

fn display_ranges<'a>(
    display_map: &'a DisplaySnapshot,
    selections: &'a SelectionsCollection,
) -> impl Iterator<Item = Range<DisplayPoint>> + 'a {
    let pending = selections.pending_anchor();
    selections
        .disjoint_anchors()
        .iter()
        .chain(pending)
        .map(move |s| s.start.to_display_point(display_map)..s.end.to_display_point(display_map))
}

pub fn deploy_context_menu(
    editor: &mut Editor,
    position: Option<Point<Pixels>>,
    point: DisplayPoint,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    if !editor.is_focused(window) {
        window.focus(&editor.focus_handle(cx), cx);
    }

    let display_map = editor.display_snapshot(cx);
    let source_anchor = display_map.display_point_to_anchor(point, text::Bias::Right);
    let context_menu = if let Some(custom) = editor.custom_context_menu.take() {
        let menu = custom(editor, point, window, cx);
        editor.custom_context_menu = Some(custom);
        let Some(menu) = menu else {
            return;
        };
        menu
    } else {
        // Don't show context menu for inline editors (only applies to default menu)
        if !editor.mode().is_full() {
            return;
        }

        // Don't show the context menu if there isn't a project associated with this editor
        let Some(project) = editor.project.clone() else {
            return;
        };

        let snapshot = editor.snapshot(window, cx);
        let display_map = editor.display_snapshot(cx);
        let buffer = snapshot.buffer_snapshot();
        let anchor = buffer.anchor_before(point.to_point(&display_map));
        if !display_ranges(&display_map, &editor.selections).any(|r| r.contains(&point)) {
            // Move the cursor to the clicked location so that dispatched actions make sense
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.clear_disjoint();
                s.set_pending_anchor_range(anchor..anchor, SelectMode::Character);
            });
        }

        let focus = window.focused(cx);
        let has_reveal_target = editor.target_file(cx).is_some();
        let file_abs_path = editor.target_file_abs_path(cx);
        let http_action = file_abs_path.as_ref().and_then(|p| {
            let ext = p.extension()?.to_str()?;
            if !matches!(ext, "http" | "rest") { return None; }
            let content = editor.buffer().read(cx).as_singleton()?.read(cx).text();
            let cursor_line = point.to_point(&display_map).row;
            parse_http_request(&content, cursor_line)
        });
        let has_selections = editor
            .selections
            .all::<PointUtf16>(&display_map)
            .into_iter()
            .any(|s| !s.is_empty());
        let has_git_repo =
            buffer
                .anchor_to_buffer_anchor(anchor)
                .is_some_and(|(buffer_anchor, _)| {
                    project
                        .read(cx)
                        .git_store()
                        .read(cx)
                        .repository_and_path_for_buffer_id(buffer_anchor.buffer_id, cx)
                        .is_some()
                });

        let evaluate_selection = window.is_action_available(&EvaluateSelectedText, cx);
        let run_to_cursor = window.is_action_available(&RunToCursor, cx);
        let format_selections = window.is_action_available(&FormatSelections, cx);
        let disable_ai = DisableAiSettings::is_ai_disabled_for_buffer(
            editor.buffer.read(cx).as_singleton().as_ref(),
            cx,
        );

        let is_markdown = editor
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|buffer| buffer.read(cx).language())
            .is_some_and(|language| language.name().as_ref() == "Markdown");

        let is_svg = editor
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|buffer| buffer.read(cx).file())
            .is_some_and(|file| {
                std::path::Path::new(file.file_name(cx))
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
            });

        let locale = WorkspaceSettings::get_global(cx).locale;
        ui::ContextMenu::build(window, cx, |menu, _window, _cx| {
            let reveal_label = t(ui::utils::reveal_in_file_manager_label(false), locale);
            let builder = menu
                .on_blur_subscription(Subscription::new(|| {}))
                .when(run_to_cursor, |builder| {
                    builder.action(t("Run to Cursor", locale), Box::new(RunToCursor))
                })
                .when(evaluate_selection && has_selections, |builder| {
                    builder.action(t("Evaluate Selection", locale), Box::new(EvaluateSelectedText))
                })
                .when(
                    run_to_cursor || (evaluate_selection && has_selections),
                    |builder| builder.separator(),
                )
                .action(t("Go to Definition", locale), Box::new(GoToDefinition))
                .action(t("Go to Declaration", locale), Box::new(GoToDeclaration))
                .action(t("Go to Type Definition", locale), Box::new(GoToTypeDefinition))
                .action(t("Go to Implementation", locale), Box::new(GoToImplementation))
                .action(
                    t("Find All References", locale),
                    Box::new(FindAllReferences::default()),
                )
                .separator()
                .action(t("Rename Symbol", locale), Box::new(Rename))
                .action(t("Format Buffer", locale), Box::new(Format))
                .when(format_selections, |cx| {
                    cx.action(t("Format Selections", locale), Box::new(FormatSelections))
                })
                .action(
                    t("Show Code Actions", locale),
                    Box::new(ToggleCodeActions {
                        deployed_from: None,
                        quick_launch: false,
                    }),
                )
                .when(!disable_ai && has_selections, |this| {
                    this.action(t("Add to Agent Thread", locale), Box::new(AddSelectionToThread))
                })
                .separator()
                .action(t("Cut", locale), Box::new(Cut))
                .action(t("Copy", locale), Box::new(Copy))
                .action(t("Copy and Trim", locale), Box::new(CopyAndTrim))
                .action(t("Paste", locale), Box::new(Paste))
                .separator()
                .action_disabled_when(
                    !has_reveal_target,
                    reveal_label,
                    Box::new(RevealInFileManager),
                )
                .when(is_markdown, |builder| {
                    builder.action(t("Open Markdown Preview", locale), Box::new(OpenMarkdownPreview))
                })
                .when(is_svg, |builder| {
                    builder.action(t("Open SVG Preview", locale), Box::new(OpenSvgPreview))
                })
                .action_disabled_when(
                    !has_reveal_target,
                    t("Open in Terminal", locale),
                    Box::new(OpenInTerminal),
                )
                .when_some(file_abs_path, |menu, path| {
                    menu.action(
                        t("Run File", locale),
                        Box::new(RunFilePathInTerminal { path }),
                    )
                })
                .when_some(http_action, |menu, action| {
                    menu.action(t("Run HTTP Request", locale), Box::new(action))
                })
                .action_disabled_when(
                    !has_git_repo,
                    t("Copy Permalink", locale),
                    Box::new(CopyPermalinkToLine),
                )
                .action_disabled_when(
                    !has_git_repo,
                    t("View File History", locale),
                    Box::new(git::FileHistory),
                );
            match focus {
                Some(focus) => builder.context(focus),
                None => builder,
            }
        })
    };

    editor.mouse_context_menu = match position {
        Some(position) => MouseContextMenu::pinned_to_editor(
            editor,
            source_anchor,
            position,
            context_menu,
            window,
            cx,
        ),
        None => {
            let character_size = editor.character_dimensions(window, cx);
            let menu_position = MenuPosition::PinnedToEditor {
                source: source_anchor,
                offset: gpui::point(character_size.em_width, character_size.line_height),
            };
            Some(MouseContextMenu::new(
                editor,
                menu_position,
                context_menu,
                window,
                cx,
            ))
        }
    };
    cx.notify();
}

pub fn shell_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    out.push_str("$'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}

pub fn parse_http_request(content: &str, cursor_line: u32) -> Option<RunHttpRequest> {
    let lines: Vec<&str> = content.lines().collect();
    let cursor = cursor_line as usize;

    // Find block boundaries (lines starting with ###)
    let mut block_starts: Vec<usize> = vec![0];
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("###") {
            block_starts.push(i + 1);
        }
    }
    block_starts.push(lines.len());

    // Find which block contains the cursor
    let block_idx = block_starts
        .windows(2)
        .position(|w| cursor >= w[0] && cursor < w[1])?;

    let block = &lines[block_starts[block_idx]..block_starts[block_idx + 1]];

    // Skip leading blank/separator lines
    let mut i = 0;
    while i < block.len() && block[i].trim().is_empty() {
        i += 1;
    }
    if i >= block.len() {
        return None;
    }

    // First non-blank line: METHOD URL [HTTP/version]
    let req_line = block[i].trim();
    if req_line.is_empty() {
        return None;
    }
    let mut parts = req_line.splitn(3, ' ');
    let method = parts.next()?.to_uppercase();
    const VALID_METHODS: &[&str] = &[
        "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "CONNECT", "TRACE",
    ];
    if !VALID_METHODS.contains(&method.as_str()) {
        return None;
    }
    let url = parts.next()?.to_string();
    i += 1;

    // Headers until blank line
    let mut headers: Vec<(String, String)> = Vec::new();
    while i < block.len() && !block[i].trim().is_empty() {
        let line = block[i].trim();
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_string();
            let val = line[colon + 1..].trim().to_string();
            headers.push((key, val));
        }
        i += 1;
    }

    // Skip blank line separator
    while i < block.len() && block[i].trim().is_empty() {
        i += 1;
    }

    // Body — stop at any ### separator line
    let body_text: String = block[i..]
        .iter()
        .take_while(|l| !l.starts_with("###"))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    let body = if body_text.is_empty() { None } else { Some(body_text) };

    let label = format!("{} {}", method, url);

    Some(RunHttpRequest { method, url, headers, body, label })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor_tests::init_test, test::editor_lsp_test_context::EditorLspTestContext};
    use indoc::indoc;

    #[gpui::test]
    async fn test_mouse_context_menu(cx: &mut gpui::TestAppContext) {
        init_test(cx, |_| {});

        let mut cx = EditorLspTestContext::new_rust(
            lsp::ServerCapabilities {
                hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            cx,
        )
        .await;

        cx.set_state(indoc! {"
            fn teˇst() {
                do_work();
            }
        "});
        let point = cx.display_point(indoc! {"
            fn test() {
                do_wˇork();
            }
        "});
        cx.editor(|editor, _window, _app| assert!(editor.mouse_context_menu.is_none()));

        cx.update_editor(|editor, window, cx| {
            deploy_context_menu(editor, Some(Default::default()), point, window, cx);

            // Assert that, even after deploying the editor's mouse context
            // menu, the editor's focus handle still contains the focused
            // element. The pane's tab bar relies on this to determine whether
            // to show the tab bar buttons and there was a small flicker when
            // deploying the mouse context menu that would cause this to not be
            // true, making it so that the buttons would disappear for a couple
            // of frames.
            assert!(editor.focus_handle.contains_focused(window, cx));
        });

        cx.assert_editor_state(indoc! {"
            fn test() {
                do_wˇork();
            }
        "});
        cx.editor(|editor, _window, _app| assert!(editor.mouse_context_menu.is_some()));
    }
}
