#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod icon;
mod store;
mod taskbar;

use icon::Style;
use serde::Serialize;
use std::{path::PathBuf, sync::Mutex};
use store::{Separator, Store};
use taskbar::{Spec, Taskbar};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};

struct AppState {
    store: Mutex<Store>,
    store_path: PathBuf,
    icon_dir: PathBuf,
    exe: PathBuf,
    taskbar: Taskbar,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
    #[serde(flatten)]
    sep: Separator,
    preview1x: String,
    preview2x: String,
}

impl AppState {
    fn icon_path(&self, sep: &Separator) -> PathBuf {
        let path = self
            .icon_dir
            .join(format!("{}-{}.ico", sep.id, sep.style.fingerprint()));
        if !path.exists() {
            if let Err(e) = icon::write_ico(&sep.style, &path) {
                eprintln!("could not write {}: {e}", path.display());
            }
        }
        path
    }

    fn spec(&self, sep: &Separator) -> Spec {
        Spec {
            id: sep.id.clone(),
            name: sep.name.clone(),
            icon_path: self.icon_path(sep),
            relaunch: format!("\"{}\" --separator {}", self.exe.display(), sep.id),
        }
    }

    fn sync(&self, sep: &Separator) {
        if sep.live {
            self.taskbar.show(self.spec(sep));
        } else {
            self.taskbar.hide(&sep.id);
        }
    }

    fn views(&self, store: &Store) -> Vec<View> {
        store
            .separators
            .iter()
            .map(|s| View {
                sep: s.clone(),
                preview1x: icon::preview_data_url(&s.style, 24),
                preview2x: icon::preview_data_url(&s.style, 48),
            })
            .collect()
    }

    /// Runs `f` on the saved list, persists it and returns the new list for the UI.
    fn edit(&self, f: impl FnOnce(&mut Store) -> Result<(), String>) -> Result<Vec<View>, String> {
        let mut store = self.store.lock().unwrap();
        f(&mut store)?;
        store
            .save(&self.store_path)
            .map_err(|e| format!("Could not save: {e}"))?;
        Ok(self.views(&store))
    }
}

fn find<'a>(store: &'a mut Store, id: &str) -> Result<&'a mut Separator, String> {
    store
        .separators
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| "That separator no longer exists.".to_string())
}

#[tauri::command]
fn list(state: State<AppState>) -> Vec<View> {
    state.views(&state.store.lock().unwrap())
}

#[tauri::command]
fn add(state: State<AppState>) -> Result<Vec<View>, String> {
    add_separator(&state)
}

fn add_separator(state: &AppState) -> Result<Vec<View>, String> {
    state.edit(|store| {
        // New separators copy the look of the most recent one.
        let style = store
            .separators
            .last()
            .map(|s| s.style.clone())
            .unwrap_or_default();
        let sep = Separator {
            id: store::new_id(),
            name: "Separator".into(),
            style,
            live: true,
        };
        state.sync(&sep);
        store.separators.push(sep);
        Ok(())
    })
}

#[tauri::command]
fn update(
    state: State<AppState>,
    id: String,
    name: String,
    style: Style,
) -> Result<Vec<View>, String> {
    state.edit(|store| {
        let sep = find(store, &id)?;
        let name = name.trim();
        sep.name = if name.is_empty() {
            "Separator".into()
        } else {
            name.into()
        };
        sep.style = style;
        state.sync(sep);
        Ok(())
    })
}

#[tauri::command]
fn set_live(state: State<AppState>, id: String, live: bool) -> Result<Vec<View>, String> {
    state.edit(|store| {
        let sep = find(store, &id)?;
        sep.live = live;
        state.sync(sep);
        Ok(())
    })
}

#[tauri::command]
fn remove(state: State<AppState>, id: String) -> Result<Vec<View>, String> {
    state.edit(|store| {
        state.taskbar.hide(&id);
        store.separators.retain(|s| s.id != id);
        Ok(())
    })
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart::is_enabled()
}

#[tauri::command]
fn set_autostart(state: State<AppState>, enabled: bool) -> Result<bool, String> {
    autostart::set(enabled, &state.exe)
        .map_err(|e| format!("Could not change startup setting: {e}"))?;
    Ok(autostart::is_enabled())
}

/// Where separator .ico files live.
///
/// Not under AppData: Explorer silently refuses taskbar relaunch icons from there and
/// shows a blank page instead (verified on Windows 11 24H2+). The folder is marked hidden
/// so it doesn't clutter the user's home folder; hidden folders work fine.
fn icon_dir(home: PathBuf) -> PathBuf {
    let root = home.join(".taskbar-separators");
    let _ = std::fs::create_dir_all(&root);
    unsafe {
        use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};
        let _ = SetFileAttributesW(
            &windows::core::HSTRING::from(root.as_os_str()),
            FILE_ATTRIBUTE_HIDDEN,
        );
    }
    root.join("icons")
}

fn show_manager(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn notify_ui(app: &AppHandle) {
    let state = app.state::<AppState>();
    let views = state.views(&state.store.lock().unwrap());
    let _ = app.emit("separators-changed", views);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // A pinned separator was clicked: there is nothing to do.
    if args.iter().any(|a| a == "--separator") {
        return;
    }
    let start_hidden = args.iter().any(|a| a == "--minimized");

    // Give the manager its own app ID. Without one, Windows matches it by .exe path to
    // any pinned separator (whose shortcut targets this same exe) and shows the manager
    // inside that separator's button. The ID matches the bundle identifier, which the
    // installer's Start-menu shortcut uses.
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            windows::core::w!("net.pasq.taskbar-separators"),
        );
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_manager(app)
        }))
        .setup(move |app| {
            let store_path = app.path().app_config_dir()?.join("separators.json");
            let handle = app.handle().clone();
            let taskbar = Taskbar::start(move |id| {
                // The user picked "Close window" on a separator's taskbar button.
                let state = handle.state::<AppState>();
                let _ = state.edit(|store| {
                    if let Ok(sep) = find(store, &id) {
                        sep.live = false;
                    }
                    Ok(())
                });
                notify_ui(&handle);
            });

            let state = AppState {
                store: Mutex::new(Store::load(&store_path)),
                store_path,
                icon_dir: icon_dir(app.path().home_dir()?),
                exe: std::env::current_exe()?,
                taskbar,
            };
            for sep in state
                .store
                .lock()
                .unwrap()
                .separators
                .iter()
                .filter(|s| s.live)
            {
                state.sync(sep);
            }
            app.manage(state);

            let open =
                MenuItem::with_id(app, "open", "Open Taskbar Separators", true, None::<&str>)?;
            let add = MenuItem::with_id(app, "add", "Add separator", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let line = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open, &add, &line, &quit])?;
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().cloned().expect("app icon"))
                .tooltip("Taskbar Separators")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_manager(app),
                    "add" => {
                        let _ = add_separator(&app.state::<AppState>());
                        notify_ui(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_manager(tray.app_handle());
                    }
                })
                .build(app)?;

            if !start_hidden {
                show_manager(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the manager keeps the app (and its separators) running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list,
            add,
            update,
            set_live,
            remove,
            get_autostart,
            set_autostart
        ])
        .run(tauri::generate_context!())
        .expect("error while running Taskbar Separators");
}
