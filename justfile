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

# NSIS installer (needs: cargo install tauri-cli --version "^2")
bundle:
    cd src-tauri; cargo tauri build

# Regenerate src-tauri/icons from assets/icon.svg (renders with headless Edge)
icon:
    node scripts/make-icon.mjs
