# Taskbar Separators

A small Rust + Tauri app that adds as many separators as you like to the Windows 11 taskbar. You can drag them anywhere among your apps and pin them so they stay put.

## How it works

Each separator is a transparent, minimized, off-screen window that has its own AppUserModelID. Because each one has its own ID, Windows gives every separator its own taskbar button, with an icon the app draws (solid, dashed, dotted or double line, or an empty gap). That button behaves like any other app button:

- **Move it:** drag it along the taskbar.
- **Keep it:** right-click it and choose **Pin to taskbar**. The window carries relaunch properties (command, name, icon), so the pinned copy keeps its look after the app exits. Clicking a pinned separator runs `taskbar-separators.exe --separator <id>`, which exits immediately.

The manager window lets you add, remove, restyle and rename separators. **Live** controls whether the app keeps a running button for a separator. You can turn it off once a separator is pinned. Closing the window sends the app to the tray, and **Quit** is in the tray menu. You can also have the app start with Windows.

### Where things are stored

| What | Where |
| --- | --- |
| Separator list | `%APPDATA%\net.pasq.taskbar-separators\separators.json` |
| Generated icons | `%USERPROFILE%\.taskbar-separators\icons\` (hidden) |

The icons can't go under AppData: Explorer ignores taskbar relaunch icons from there and shows a blank page. Each icon file's name includes a hash of its style, so Windows' icon cache never shows an old look.

### Limitations

- Windows has no public API for setting a taskbar button's position. New separators appear at the right end, and you drag them into place.
- If you restyle a separator that's pinned, the live button updates right away. The pinned copy keeps its old look until you unpin and re-pin it.
- Separators show up in Alt+Tab as blank entries while they're live.

## Build

Requires Rust (MSVC toolchain) and WebView2, which Windows 11 already includes. Node is needed only to regenerate the app icon.

```bash
just run       # debug build + launch
just test      # unit tests
just release   # optimized exe at src-tauri/target/release/
just bundle    # NSIS installer (needs: cargo install tauri-cli)
```
