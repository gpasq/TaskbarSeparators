# Run `just` to list recipes.

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

# List recipes
default:
    @just --list

# Launch the app from source
run:
    cd src-tauri; cargo run

# Run the unit tests
test:
    cd src-tauri; cargo test

# Clippy with warnings as errors
lint:
    cd src-tauri; cargo clippy --all-targets -- -D warnings

# Optimized exe at src-tauri/target/release/taskbar-separators.exe
release:
    cd src-tauri; cargo build --release

install_dir := env_var("LOCALAPPDATA") + '\Programs\Taskbar Separators'
start_menu_link := env_var("APPDATA") + '\Microsoft\Windows\Start Menu\Programs\Taskbar Separators.lnk'

# Install to %LOCALAPPDATA%\Programs\Taskbar Separators with a Start-menu shortcut, then launch it
install: release
    Get-Process taskbar-separators -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 1500
    New-Item -ItemType Directory -Force '{{install_dir}}' | Out-Null; Copy-Item -Force src-tauri/target/release/taskbar-separators.exe '{{install_dir}}'
    $s = (New-Object -ComObject WScript.Shell).CreateShortcut('{{start_menu_link}}'); $s.TargetPath = '{{install_dir}}\taskbar-separators.exe'; $s.Description = 'Add movable separators to the taskbar'; $s.Save()
    $run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'; if (Get-ItemProperty $run -Name TaskbarSeparators -ErrorAction SilentlyContinue) { Set-ItemProperty $run -Name TaskbarSeparators -Value ('"{{install_dir}}\taskbar-separators.exe" --minimized') }
    Start-Process '{{install_dir}}\taskbar-separators.exe'
    @echo "Installed to {{install_dir}}. Re-pin any separators pinned from a source build."

# Remove the installed copy, its Start-menu shortcut and the start-with-Windows entry (keeps your separator list)
uninstall:
    Get-Process taskbar-separators -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 1500
    Remove-Item -Recurse -Force '{{install_dir}}', '{{start_menu_link}}' -ErrorAction SilentlyContinue
    Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name TaskbarSeparators -ErrorAction SilentlyContinue
    @echo "Uninstalled. Unpin any pinned separators from the taskbar."

# NSIS installer (needs: cargo install tauri-cli --version "^2")
bundle:
    cd src-tauri; cargo tauri build

# Regenerate src-tauri/icons from assets/icon.svg (renders with headless Edge)
icon:
    node scripts/make-icon.mjs
