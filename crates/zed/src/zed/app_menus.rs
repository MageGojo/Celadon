use collab_ui::collab_panel;
use gpui::{App, Menu, MenuItem, OsAction};
use i18n::t;
use release_channel::ReleaseChannel;
use terminal_view::terminal_panel;
use workspace::WorkspaceSettings;
use zed_actions::{debug_panel, dev};

pub fn app_menus(cx: &mut App) -> Vec<Menu> {
    use settings::Settings as _;
    use zed_actions::Quit;

    let locale = WorkspaceSettings::get_global(cx).locale;

    let mut view_items = vec![
        MenuItem::action(
            t("Zoom In", locale),
            zed_actions::IncreaseBufferFontSize { persist: false },
        ),
        MenuItem::action(
            t("Zoom Out", locale),
            zed_actions::DecreaseBufferFontSize { persist: false },
        ),
        MenuItem::action(
            t("Reset Zoom", locale),
            zed_actions::ResetBufferFontSize { persist: false },
        ),
        MenuItem::action(
            t("Reset All Zoom", locale),
            zed_actions::ResetAllZoom { persist: false },
        ),
        MenuItem::separator(),
        MenuItem::action(t("Toggle Left Dock", locale), workspace::ToggleLeftDock),
        MenuItem::action(t("Toggle Right Dock", locale), workspace::ToggleRightDock),
        MenuItem::action(t("Toggle Bottom Dock", locale), workspace::ToggleBottomDock),
        MenuItem::action(t("Toggle All Docks", locale), workspace::ToggleAllDocks),
        MenuItem::submenu(Menu {
            name: t("Editor Layout", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::action(t("Split Up", locale), workspace::SplitUp::default()),
                MenuItem::action(t("Split Down", locale), workspace::SplitDown::default()),
                MenuItem::action(t("Split Left", locale), workspace::SplitLeft::default()),
                MenuItem::action(t("Split Right", locale), workspace::SplitRight::default()),
            ],
        }),
        MenuItem::separator(),
        MenuItem::action(t("Project Panel", locale), zed_actions::project_panel::ToggleFocus),
        MenuItem::action(t("Outline Panel", locale), outline_panel::ToggleFocus),
        MenuItem::action(t("Collab Panel", locale), collab_panel::ToggleFocus),
        MenuItem::action(t("Terminal Panel", locale), terminal_panel::ToggleFocus),
        MenuItem::action(t("Debugger Panel", locale), debug_panel::ToggleFocus),
        MenuItem::separator(),
        MenuItem::action(t("Diagnostics", locale), diagnostics::Deploy),
        MenuItem::separator(),
    ];

    if ReleaseChannel::try_global(cx) == Some(ReleaseChannel::Dev) {
        view_items.push(MenuItem::action(
            t("Toggle GPUI Inspector", locale),
            dev::ToggleInspector,
        ));
        view_items.push(MenuItem::separator());
    }

    vec![
        Menu {
            name: "Zed".into(),
            disabled: false,
            items: vec![
                MenuItem::action(t("About Zed", locale), zed_actions::About),
                MenuItem::action(t("Check for Updates", locale), auto_update::Check),
                MenuItem::separator(),
                MenuItem::submenu(Menu::new(t("Settings", locale)).items([
                    MenuItem::action(t("Open Settings", locale), zed_actions::OpenSettings),
                    MenuItem::action(t("Open Settings File", locale), super::OpenSettingsFile),
                    MenuItem::action(t("Open Project Settings", locale), zed_actions::OpenProjectSettings),
                    MenuItem::action(t("Open Project Settings File", locale), super::OpenProjectSettingsFile),
                    MenuItem::action(t("Open Default Settings", locale), super::OpenDefaultSettings),
                    MenuItem::separator(),
                    MenuItem::action(t("Open Keymap", locale), zed_actions::OpenKeymap),
                    MenuItem::action(t("Open Keymap File", locale), zed_actions::OpenKeymapFile),
                    MenuItem::action(t("Open Default Key Bindings", locale), zed_actions::OpenDefaultKeymap),
                    MenuItem::separator(),
                    MenuItem::action(
                        t("Select Theme...", locale),
                        zed_actions::theme_selector::Toggle::default(),
                    ),
                    MenuItem::action(
                        t("Select Icon Theme...", locale),
                        zed_actions::icon_theme_selector::Toggle::default(),
                    ),
                ])),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(t("Extensions", locale), zed_actions::Extensions::default()),
                #[cfg(not(target_os = "windows"))]
                MenuItem::action(t("Install CLI", locale), install_cli::InstallCliBinary),
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(t("Hide Zed", locale), super::Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action(t("Hide Others", locale), super::HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action(t("Show All", locale), super::ShowAll),
                MenuItem::separator(),
                MenuItem::action(t("Quit Zed", locale), Quit),
            ],
        },
        Menu {
            name: t("File", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::action(t("New", locale), workspace::NewFile),
                MenuItem::action(t("New Window", locale), workspace::NewWindow),
                MenuItem::separator(),
                #[cfg(not(target_os = "macos"))]
                MenuItem::action(t("Open File...", locale), workspace::OpenFiles),
                MenuItem::action(
                    if cfg!(not(target_os = "macos")) {
                        t("Open Folder...", locale)
                    } else {
                        t("Open…", locale)
                    },
                    workspace::Open::default(),
                ),
                MenuItem::action(
                    t("Open Recent...", locale),
                    zed_actions::OpenRecent {
                        create_new_window: false,
                    },
                ),
                MenuItem::action(
                    t("Open Remote...", locale),
                    zed_actions::OpenRemote {
                        create_new_window: false,
                        from_existing_connection: false,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(t("Add Folder to Project…", locale), workspace::AddFolderToProject),
                MenuItem::separator(),
                MenuItem::action(t("Save", locale), workspace::Save { save_intent: None }),
                MenuItem::action(t("Save As…", locale), workspace::SaveAs),
                MenuItem::action(t("Save All", locale), workspace::SaveAll { save_intent: None }),
                MenuItem::separator(),
                MenuItem::action(
                    t("Close Editor", locale),
                    workspace::CloseActiveItem {
                        save_intent: None,
                        close_pinned: true,
                    },
                ),
                MenuItem::action(t("Close Project", locale), workspace::CloseProject),
                MenuItem::action(t("Close Window", locale), workspace::CloseWindow),
            ],
        },
        Menu {
            name: t("Edit", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::os_action(t("Undo", locale), editor::actions::Undo, OsAction::Undo),
                MenuItem::os_action(t("Redo", locale), editor::actions::Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action(t("Cut", locale), editor::actions::Cut, OsAction::Cut),
                MenuItem::os_action(t("Copy", locale), editor::actions::Copy, OsAction::Copy),
                MenuItem::action(t("Copy and Trim", locale), editor::actions::CopyAndTrim),
                MenuItem::os_action(t("Paste", locale), editor::actions::Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::action(t("Find", locale), search::buffer_search::Deploy::find()),
                MenuItem::action(t("Find in Project", locale), workspace::DeploySearch::default()),
                MenuItem::separator(),
                MenuItem::action(
                    t("Toggle Line Comment", locale),
                    editor::actions::ToggleComments::default(),
                ),
            ],
        },
        Menu {
            name: t("Selection", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::os_action(
                    t("Select All", locale),
                    editor::actions::SelectAll,
                    OsAction::SelectAll,
                ),
                MenuItem::action(t("Expand Selection", locale), editor::actions::SelectLargerSyntaxNode),
                MenuItem::action(t("Shrink Selection", locale), editor::actions::SelectSmallerSyntaxNode),
                MenuItem::action(t("Select Next Sibling", locale), editor::actions::SelectNextSyntaxNode),
                MenuItem::action(
                    t("Select Previous Sibling", locale),
                    editor::actions::SelectPreviousSyntaxNode,
                ),
                MenuItem::separator(),
                MenuItem::action(
                    t("Add Cursor Above", locale),
                    editor::actions::AddSelectionAbove {
                        skip_soft_wrap: true,
                    },
                ),
                MenuItem::action(
                    t("Add Cursor Below", locale),
                    editor::actions::AddSelectionBelow {
                        skip_soft_wrap: true,
                    },
                ),
                MenuItem::action(
                    t("Select Next Occurrence", locale),
                    editor::actions::SelectNext {
                        replace_newest: false,
                    },
                ),
                MenuItem::action(
                    t("Select Previous Occurrence", locale),
                    editor::actions::SelectPrevious {
                        replace_newest: false,
                    },
                ),
                MenuItem::action(t("Select All Occurrences", locale), editor::actions::SelectAllMatches),
                MenuItem::separator(),
                MenuItem::action(t("Move Line Up", locale), editor::actions::MoveLineUp),
                MenuItem::action(t("Move Line Down", locale), editor::actions::MoveLineDown),
                MenuItem::action(t("Duplicate Selection", locale), editor::actions::DuplicateLineDown),
            ],
        },
        Menu {
            name: t("View", locale).into(),
            disabled: false,
            items: view_items,
        },
        Menu {
            name: t("Go", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::action(t("Back", locale), workspace::GoBack),
                MenuItem::action(t("Forward", locale), workspace::GoForward),
                MenuItem::separator(),
                MenuItem::action(t("Command Palette...", locale), zed_actions::command_palette::Toggle),
                MenuItem::separator(),
                MenuItem::action(t("Go to File...", locale), workspace::ToggleFileFinder::default()),
                // MenuItem::action("Go to Symbol in Project", project_symbols::Toggle),
                MenuItem::action(
                    t("Go to Symbol in Editor...", locale),
                    zed_actions::outline::ToggleOutline,
                ),
                MenuItem::action(t("Go to Line/Column...", locale), editor::actions::ToggleGoToLine),
                MenuItem::separator(),
                MenuItem::action(t("Go to Definition", locale), editor::actions::GoToDefinition),
                MenuItem::action(t("Go to Declaration", locale), editor::actions::GoToDeclaration),
                MenuItem::action(t("Go to Type Definition", locale), editor::actions::GoToTypeDefinition),
                MenuItem::action(
                    t("Find All References", locale),
                    editor::actions::FindAllReferences::default(),
                ),
                MenuItem::separator(),
                MenuItem::action(t("Next Problem", locale), editor::actions::GoToDiagnostic::default()),
                MenuItem::action(
                    t("Previous Problem", locale),
                    editor::actions::GoToPreviousDiagnostic::default(),
                ),
            ],
        },
        Menu {
            name: t("Run", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    t("Spawn Task", locale),
                    zed_actions::Spawn::ViaModal {
                        reveal_target: None,
                    },
                ),
                MenuItem::action(t("Start Debugger", locale), debugger_ui::Start),
                MenuItem::separator(),
                MenuItem::action(t("Edit tasks.json...", locale), crate::zed::OpenProjectTasks),
                MenuItem::action(t("Edit debug.json...", locale), zed_actions::OpenProjectDebugTasks),
                MenuItem::separator(),
                MenuItem::action(t("Continue", locale), debugger_ui::Continue),
                MenuItem::action(t("Step Over", locale), debugger_ui::StepOver),
                MenuItem::action(t("Step Into", locale), debugger_ui::StepInto),
                MenuItem::action(t("Step Out", locale), debugger_ui::StepOut),
                MenuItem::separator(),
                MenuItem::action(t("Toggle Breakpoint", locale), editor::actions::ToggleBreakpoint),
                MenuItem::action(t("Edit Breakpoint", locale), editor::actions::EditLogBreakpoint),
                MenuItem::action(t("Clear All Breakpoints", locale), debugger_ui::ClearAllBreakpoints),
            ],
        },
        Menu {
            name: t("Window", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::action(t("Minimize", locale), super::Minimize),
                MenuItem::action(t("Zoom", locale), super::Zoom),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: t("Help", locale).into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    t("View Release Notes Locally", locale),
                    auto_update_ui::ViewReleaseNotesLocally,
                ),
                MenuItem::action(t("View Telemetry", locale), zed_actions::OpenTelemetryLog),
                MenuItem::action(t("View Dependency Licenses", locale), zed_actions::OpenLicenses),
                MenuItem::action(t("Show Welcome", locale), onboarding::ShowWelcome),
                MenuItem::separator(),
                MenuItem::action(t("File Bug Report...", locale), zed_actions::feedback::FileBugReport),
                MenuItem::action(t("Request Feature...", locale), zed_actions::feedback::RequestFeature),
                MenuItem::action(t("Email Us...", locale), zed_actions::feedback::EmailZed),
                MenuItem::separator(),
                MenuItem::action(
                    t("Documentation", locale),
                    super::OpenBrowser {
                        url: "https://zed.dev/docs".into(),
                    },
                ),
                MenuItem::action(t("Zed Repository", locale), feedback::OpenZedRepo),
                MenuItem::action(
                    t("Zed Twitter", locale),
                    super::OpenBrowser {
                        url: "https://twitter.com/zeddotdev".into(),
                    },
                ),
                MenuItem::action(
                    t("Join the Team", locale),
                    super::OpenBrowser {
                        url: "https://zed.dev/jobs".into(),
                    },
                ),
            ],
        },
    ]
}
