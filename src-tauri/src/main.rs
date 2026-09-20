#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod selection;
mod settings;
mod watcher;

use std::{
    sync::{atomic::Ordering, Mutex},
    time::{Duration, Instant},
};

use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, State, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use settings::{Settings, Store};

/// Whatever text the panel is currently working on.
struct Held(Mutex<String>);

/// When the panel was last shown. macOS can report a blur in the same breath
/// as the show, which had the panel hiding itself the instant it appeared.
struct ShownAt(Mutex<Instant>);

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn ask_model(
    app: AppHandle,
    store: State<'_, Store>,
    system: String,
    user: String,
) -> Result<(), String> {
    let cfg = store.get();
    tauri::async_runtime::spawn(model::stream(app, cfg.host, cfg.model, system, user));
    Ok(())
}

#[tauri::command]
fn get_settings(store: State<'_, Store>) -> Settings {
    store.get()
}

#[tauri::command]
fn save_settings(store: State<'_, Store>, next: Settings) {
    *store.inner.lock().unwrap() = next;
    store.save();
}

#[tauri::command]
async fn list_models(store: State<'_, Store>) -> Result<Vec<String>, String> {
    let host = store.get().host;
    Ok(model::list(&host).await)
}

#[tauri::command]
fn held_text(held: State<'_, Held>) -> String {
    held.0.lock().unwrap().clone()
}

/// Called when you click the bead, or accept an offer.
#[tauri::command]
fn open_panel(app: AppHandle, text: String) {
    if !text.is_empty() {
        *app.state::<Held>().0.lock().unwrap() = text.clone();
    }
    let text = app.state::<Held>().0.lock().unwrap().clone();
    show_panel_at_cursor(&app, text);
}

#[tauri::command]
fn dismiss_panel(app: AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    let _ = app.emit("offer-cleared", ());
}

#[tauri::command]
fn replace_selection(app: AppHandle, text: String) -> Result<(), String> {
    // Hide first, so focus returns to the app underneath — otherwise the paste
    // lands in clicky's own input box.
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    selection::replace(&text)
}

/// The bead ignores the mouse while idle, so it never eats a click you meant
/// for the app underneath. It only becomes clickable when it has something
/// to offer you.
#[tauri::command]
fn bead_clickthrough(app: AppHandle, on: bool) {
    if let Some(b) = app.get_webview_window("bead") {
        let _ = b.set_ignore_cursor_events(on);
    }
    // Clickable and frozen go together: a window that keeps moving between
    // your mousedown and mouseup eats the click.
    if let Some(w) = app.try_state::<watcher::Watcher>() {
        w.frozen.store(!on, Ordering::Relaxed);
    }
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

// ---------------------------------------------------------------------------
// Showing the panel
// ---------------------------------------------------------------------------

pub fn show_panel_at_cursor(app: &AppHandle, text: String) {
    show_panel(app, text, true, false)
}

/// `focus` hands the panel your keyboard. `auto` tells the frontend this one
/// appeared on its own, so it can close itself again if you ignore it.
pub fn show_panel(app: &AppHandle, text: String, focus: bool, auto: bool) {
    let Some(win) = app.get_webview_window("panel") else {
        return;
    };

    if let Ok(cursor) = app.cursor_position() {
        let size = win.outer_size().unwrap_or_default();
        let (mut x, mut y) = (cursor.x + 16.0, cursor.y + 24.0);

        if let Ok(Some(mon)) = win.current_monitor() {
            let m = mon.position();
            let s = mon.size();
            let right = (m.x + s.width as i32) as f64;
            let bottom = (m.y + s.height as i32) as f64;

            x = x
                .min(right - size.width as f64 - 12.0)
                .max(m.x as f64 + 12.0);
            // Near the bottom edge, flip above the cursor rather than run off.
            if y + size.height as f64 > bottom - 12.0 {
                y = cursor.y - size.height as f64 - 20.0;
            }
            y = y.max(m.y as f64 + 12.0);
        }
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    *app.state::<ShownAt>().0.lock().unwrap() = Instant::now();
    let _ = win.show();
    if focus {
        let _ = win.set_focus();
    }

    let close_after = app.state::<Store>().get().auto_close_ms;
    let _ = app.emit(
        "summoned",
        serde_json::json!({ "text": text, "auto": auto, "closeAfter": close_after }),
    );
}

/// The global hotkey: grab the selection right now, whether or not the
/// watcher noticed it.
fn summon(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("panel") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
            return;
        }
    }

    if let Some(w) = app.try_state::<watcher::Watcher>() {
        w.paused.store(true, Ordering::Relaxed);
        let text = selection::capture();
        *w.last_offer.lock().unwrap() = text.clone();
        w.paused.store(false, Ordering::Relaxed);
        *app.state::<Held>().0.lock().unwrap() = text.clone();
        show_panel_at_cursor(app, text);
    }
}

// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        summon(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            ask_model,
            get_settings,
            save_settings,
            list_models,
            held_text,
            open_panel,
            dismiss_panel,
            replace_selection,
            bead_clickthrough,
            quit
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // ---- settings ----
            let store = Store::load(&handle);
            {
                let mut s = store.inner.lock().unwrap();
                settings::apply_env(&mut s);
            }
            let cfg = store.get();
            app.manage(store);
            app.manage(Held(Mutex::new(String::new())));
            app.manage(ShownAt(Mutex::new(Instant::now())));

            // A menu-bar utility: no dock icon, no Cmd-Tab entry, and it does
            // not deactivate the app you're working in when it shows a window.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // ---- hotkey ----
            let shortcut = if cfg.hotkey.is_empty() {
                Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space)
            } else {
                cfg.hotkey.parse().unwrap_or_else(|_| {
                    Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space)
                })
            };
            if let Err(e) = app.global_shortcut().register(shortcut) {
                eprintln!("clicky: couldn't register the hotkey: {e}");
            }

            // ---- tray ----
            let follow = CheckMenuItemBuilder::with_id("follow", "Follow cursor")
                .checked(cfg.follow_cursor)
                .build(app)?;
            let watch = CheckMenuItemBuilder::with_id("watch", "Offer on selection")
                .checked(cfg.watch_selection)
                .build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&MenuItemBuilder::with_id("open", "Open clicky").build(app)?)
                .separator()
                .item(&follow)
                .item(&watch)
                .separator()
                .item(&MenuItemBuilder::with_id("config", "Edit settings…").build(app)?)
                .item(&MenuItemBuilder::with_id("quit", "Quit clicky").build(app)?)
                .build()?;

            TrayIconBuilder::with_id("tray")
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/tray.png"
                ))?)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| {
                    let store = app.state::<Store>();
                    match event.id().as_ref() {
                        "open" => show_panel_at_cursor(app, String::new()),
                        "follow" => {
                            let mut s = store.inner.lock().unwrap();
                            s.follow_cursor = !s.follow_cursor;
                            let on = s.follow_cursor;
                            drop(s);
                            store.save();
                            if let Some(b) = app.get_webview_window("bead") {
                                let _ = if on { b.show() } else { b.hide() };
                            }
                        }
                        "watch" => {
                            let mut s = store.inner.lock().unwrap();
                            s.watch_selection = !s.watch_selection;
                            drop(s);
                            store.save();
                        }
                        "config" => {
                            use tauri_plugin_opener::OpenerExt;
                            let _ = app
                                .opener()
                                .open_path(store.path.to_string_lossy(), None::<&str>);
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    }
                })
                .build(app)?;

            // ---- windows ----
            if let Some(bead) = app.get_webview_window("bead") {
                let _ = bead.set_visible_on_all_workspaces(true);
                let _ = bead.set_ignore_cursor_events(true);
                if cfg.follow_cursor {
                    let _ = bead.show();
                }
            }

            if let Some(panel) = app.get_webview_window("panel") {
                // Both windows need to exist on every Space, or they vanish
                // the moment you swipe to another desktop.
                let _ = panel.set_visible_on_all_workspaces(true);

                let p = panel.clone();
                let handle2 = app.handle().clone();
                panel.on_window_event(move |e| {
                    // Click away and the panel goes. Keeps a HUD from becoming
                    // a window you have to manage — but ignore the blur that
                    // can arrive while the window is still being shown.
                    if let WindowEvent::Focused(false) = e {
                        let shown = *handle2.state::<ShownAt>().0.lock().unwrap();
                        if shown.elapsed() > Duration::from_millis(600) {
                            let _ = p.hide();
                        }
                    }
                });
            }

            // ---- the loop ----
            app.manage(watcher::Watcher::start(handle));

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("clicky failed to start")
        .run(|_app, event| {
            // Closing the last window shouldn't quit a menu-bar app.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
