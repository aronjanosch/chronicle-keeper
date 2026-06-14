// Phase 14D: native top menu bar. Items carry command ids the frontend already
// understands (commands.js); on click the id is emitted as a `ck-menu` event.
// Zoom is the exception — handled here directly on the webview.
use std::sync::Mutex;

use tauri::menu::{MenuBuilder, MenuEvent, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_opener::OpenerExt;

const DOCS_URL: &str = "https://aronjanosch.github.io/chronicle-keeper/";

/// Format items that only apply while the page editor is mounted; the
/// frontend toggles them via the `set_format_enabled` command.
pub struct EditorMenuItems(Vec<MenuItem<Wry>>);

pub struct ZoomLevel(Mutex<f64>);

#[tauri::command]
pub fn set_format_enabled(app: AppHandle, enabled: bool) {
    if let Some(items) = app.try_state::<EditorMenuItems>() {
        for item in &items.0 {
            let _ = item.set_enabled(enabled);
        }
    }
}

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let mi = |id: &str, label: &str, accel: Option<&str>| -> tauri::Result<MenuItem<Wry>> {
        let mut b = MenuItemBuilder::with_id(id, label);
        if let Some(a) = accel {
            b = b.accelerator(a);
        }
        b.build(app)
    };

    #[cfg(target_os = "macos")]
    let app_menu = SubmenuBuilder::new(app, "Chronicle Keeper")
        .about(None)
        .separator()
        .item(&mi("settings", "Settings…", Some("CmdOrCtrl+,"))?)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    #[allow(unused_mut)] // mutated only on non-macOS (settings + quit)
    let mut file = SubmenuBuilder::new(app, "File")
        .item(&mi("new-page", "New Page", Some("CmdOrCtrl+N"))?)
        .item(&mi("new-folder", "New Folder", None)?)
        .item(&mi(
            "quick-capture",
            "Quick Capture",
            Some("CmdOrCtrl+Shift+J"),
        )?)
        .separator()
        .item(&mi("import", "Import Notes…", None)?)
        .item(&mi("export-world", "Export World…", None)?)
        .separator()
        .item(&mi("go-library", "All Worlds", None)?)
        .separator()
        // Close Tab owns CmdOrCtrl+W (15D), so Close Window is a custom item on
        // Shift — the predefined one would grab CmdOrCtrl+W for itself.
        .item(&mi("tab-close", "Close Tab", Some("CmdOrCtrl+W"))?)
        .item(&mi(
            "tab-reopen",
            "Reopen Closed Tab",
            Some("CmdOrCtrl+Shift+T"),
        )?)
        .item(&mi(
            "close-window",
            "Close Window",
            Some("CmdOrCtrl+Shift+W"),
        )?);
    #[cfg(not(target_os = "macos"))]
    {
        file = file
            .separator()
            .item(&mi("settings", "Settings…", Some("CmdOrCtrl+,"))?)
            .separator()
            .quit();
    }
    let file = file.build()?;

    let edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .separator()
        .item(&mi("find", "Find in Page", Some("CmdOrCtrl+F"))?)
        .item(&mi(
            "search-world",
            "Search World",
            Some("CmdOrCtrl+Shift+F"),
        )?)
        .build()?;

    let f_bold = mi("fmt-bold", "Bold", Some("CmdOrCtrl+B"))?;
    let f_italic = mi("fmt-italic", "Italic", Some("CmdOrCtrl+I"))?;
    let f_code = mi("fmt-code", "Code", None)?;
    let f_highlight = mi("fmt-highlight", "Highlight", None)?;
    let f_wikilink = mi("fmt-wikilink", "Wikilink", Some("CmdOrCtrl+L"))?;
    let f_h1 = mi("fmt-h1", "Heading 1", Some("CmdOrCtrl+Alt+1"))?;
    let f_h2 = mi("fmt-h2", "Heading 2", Some("CmdOrCtrl+Alt+2"))?;
    let f_h3 = mi("fmt-h3", "Heading 3", Some("CmdOrCtrl+Alt+3"))?;
    let f_list = mi("fmt-list", "Bullet List", None)?;
    let f_quote = mi("fmt-quote", "Quote", None)?;
    let f_callout = mi("fmt-callout", "Callout", None)?;
    let format = SubmenuBuilder::new(app, "Format")
        .item(&f_bold)
        .item(&f_italic)
        .item(&f_code)
        .item(&f_highlight)
        .item(&f_wikilink)
        .separator()
        .item(&f_h1)
        .item(&f_h2)
        .item(&f_h3)
        .separator()
        .item(&f_list)
        .item(&f_quote)
        .item(&f_callout)
        .build()?;

    let view = SubmenuBuilder::new(app, "View")
        .item(&mi("palette", "Command Palette…", Some("CmdOrCtrl+K"))?)
        .item(&mi("quick-open", "Quick Open…", Some("CmdOrCtrl+P"))?)
        .separator()
        .item(&mi(
            "toggle-rail",
            "Toggle Side Panel",
            Some("CmdOrCtrl+Shift+K"),
        )?)
        .item(&mi("zen", "Zen Mode", None)?)
        .separator()
        .item(&mi("go-overview", "World Overview", None)?)
        .item(&mi("go-codex", "Codex", None)?)
        .item(&mi("go-atlas", "Atlas", None)?)
        .item(&mi("go-timeline", "Timeline", None)?)
        .item(&mi("go-graph", "Graph", None)?)
        .item(&mi("go-sessions", "Sessions", None)?)
        .item(&mi("go-keeper", "The Keeper", None)?)
        .separator()
        .item(&mi("nav-back", "Back", Some("CmdOrCtrl+["))?)
        .item(&mi("nav-forward", "Forward", Some("CmdOrCtrl+]"))?)
        .separator()
        .item(&mi("tab-next", "Next Tab", Some("CmdOrCtrl+Shift+]"))?)
        .item(&mi("tab-prev", "Previous Tab", Some("CmdOrCtrl+Shift+["))?)
        .separator()
        .item(&mi("zoom-in", "Zoom In", Some("CmdOrCtrl+="))?)
        .item(&mi("zoom-out", "Zoom Out", Some("CmdOrCtrl+-"))?)
        .item(&mi("zoom-reset", "Actual Size", Some("CmdOrCtrl+0"))?)
        .separator()
        .fullscreen()
        .build()?;

    let window = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .build()?;

    let help = SubmenuBuilder::new(app, "Help")
        .item(&mi("shortcuts", "Keyboard Shortcuts", Some("CmdOrCtrl+/"))?)
        .item(&mi("docs", "Chronicle Keeper Website", None)?)
        .build()?;

    let mut mb = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    {
        mb = mb.item(&app_menu);
    }
    let menu = mb
        .item(&file)
        .item(&edit)
        .item(&format)
        .item(&view)
        .item(&window)
        .item(&help)
        .build()?;
    app.set_menu(menu)?;

    let editor_items = vec![
        f_bold,
        f_italic,
        f_code,
        f_highlight,
        f_wikilink,
        f_h1,
        f_h2,
        f_h3,
        f_list,
        f_quote,
        f_callout,
    ];
    for item in &editor_items {
        let _ = item.set_enabled(false);
    }
    app.manage(EditorMenuItems(editor_items));
    app.manage(ZoomLevel(Mutex::new(1.0)));
    Ok(())
}

pub fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().0.as_str();
    match id {
        "zoom-in" | "zoom-out" | "zoom-reset" => {
            let zoom = app.state::<ZoomLevel>();
            let mut z = zoom.0.lock().unwrap();
            *z = match id {
                "zoom-in" => (*z + 0.1).min(3.0),
                "zoom-out" => (*z - 0.1).max(0.5),
                _ => 1.0,
            };
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_zoom(*z);
            }
        }
        "docs" => {
            let _ = app.opener().open_url(DOCS_URL, None::<&str>);
        }
        "close-window" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.close();
            }
        }
        _ => {
            let _ = app.emit("ck-menu", id);
        }
    }
}
