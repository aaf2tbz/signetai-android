# Signet Android

Native Android APK wrapping the Signet daemon and dashboard using Tauri 2.

## Architecture

- **Tauri 2 Runtime**: Native WebView hosting the Signet dashboard
- **signet-daemon (sidecar)**: Rust binary running as a child process inside the APK
  - HTTP server on `localhost:3850`
  - SQLite storage in app's internal data directory
  - Embedding fetch, file watching, memory pipeline
- **Android Integration**: Share target, foreground service, notifications

## Prerequisites

1. **Android Studio** with SDK (API 28+)
2. **Android NDK** 26.x
3. **Rust Android target**: `rustup target add aarch64-linux-android`
4. **Java JDK 17+**
5. **Tauri CLI 2**: `cargo install tauri-cli --version "^2"`

### Environment Setup

```bash
export ANDROID_HOME=$HOME/Library/Android/sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/26.1.10909125
export JAVA_HOME=$(/usr/libexec/java_home -v 17)
```

## Building

### 1. Build daemon for Android

```bash
cd daemon-rs
cargo build --release --target aarch64-linux-android
```

If `daemon-rs` is symlinked from the main signetai repo, the binary will be at:
`daemon-rs/target/aarch64-linux-android/release/signet-daemon`

### 2. Stage daemon sidecar

```bash
export SIGNET_DAEMON_BIN=daemon-rs/target/aarch64-linux-android/release/signet-daemon
node scripts/stage-daemon.mjs
```

### 3. Initialize Tauri Android (first time only)

```bash
cargo tauri android init
```

### 4. Build APK

```bash
# Debug build (for development)
cargo tauri android build --debug

# Release build
cargo tauri android build
```

Output: `src-tauri/gen/android/app/build/outputs/apk/`

### 5. Install

```bash
adb install -r src-tauri/gen/android/app/build/outputs/apk/debug/app-debug.apk
```

## Development

For now, the dashboard is a lightweight HTML page. To use the full SvelteKit dashboard:

1. Replace the `index.html` with the built output from `packages/cli/dashboard`
2. Update `tauri.conf.json` `build.frontendDist` to point to the dashboard dist directory

## Share Target

The app registers as an Android share target for `text/plain`. Users can share
text from any app (Claude, ChatGPT, etc.) directly to Signet for ingestion.

## Project Structure

```
src-tauri/
  src/
    main.rs           — Entry point
    lib.rs            — Tauri app setup (Android-aware)
    commands.rs       — Tauri commands (no tray deps)
    daemon.rs         — Daemon lifecycle management
    platform/
      mod.rs          — DaemonManager trait + factory
      android.rs      — Android daemon manager (extract, spawn, manage)
      linux.rs        — Linux stub (dev)
      macos.rs        — macOS stub (dev)
      windows.rs      — Windows stub (dev)
  capabilities/       — Tauri permissions
  binaries/           — Staged daemon sidecar binary
  icons/              — App icons
  tauri.conf.json     — Tauri configuration
scripts/
  stage-daemon.mjs    — Daemon sidecar staging script
```
