# Signet Android

Native Android APK wrapping the Signet daemon and SvelteKit dashboard using Tauri 2. Self-contained — the daemon binary, llama-server, and embedding model are bundled inside the APK and extracted on first launch. On-device inference via llama.cpp for embeddings and extraction.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  APK (255MB debug / ~65MB release)              │
│                                                  │
│  ┌──────────┐  ┌──────────────┐  ┌───────────┐ │
│  │ Tauri 2  │  │ signet-daemon│  │llama-server│ │
│  │ WebView  │──│ :3850 HTTP   │──│ :8080      │ │
│  │ Dashboard│  │ pipeline/API │  │ embeddings │ │
│  └──────────┘  └──────────────┘  └─────┬─────┘ │
│                                        │        │
│                                  ┌─────┴─────┐  │
│                                  │ GGUF model │  │
│                                  │ (95MB Q4)  │  │
│                                  └───────────┘  │
└─────────────────────────────────────────────────┘
```

- **Tauri 2 Runtime**: Native WebView hosting the full SvelteKit dashboard
- **signet-daemon**: Rust HTTP server on `:3850` — memory pipeline, API, SQLite storage
- **llama-server**: llama.cpp server on `:8080` — on-device embedding and LLM inference
  - OpenAI-compatible API (`/v1/embeddings`, `/v1/chat/completions`)
  - Uses `nomic-embed-text-v1.5` Q4_K_M (95MB) for embeddings
  - Can load extraction LLMs (Qwen 0.6B-4B) for on-device extraction
- **Kotlin layer**: Extracts binaries + model from APK assets, creates config, starts services

## Flow on Device

1. App launches → `MainActivity.onCreate()`
2. Kotlin extracts binaries from APK `assets/` to `files/.agents/bin/` (first launch only)
3. Kotlin extracts GGUF model to `files/.agents/models/`
4. Creates `files/.agents/agent.yaml` with `llama-cpp` as provider
5. Starts foreground service
6. Rust Tauri lib spawns **llama-server** with `--embedding` flag and the GGUF model
7. Health check polls `localhost:8080` until llama-server responds (up to 15s)
8. Spawns **signet-daemon** with `SIGNET_PATH` pointing to `.agents/`
9. Health check polls `localhost:3850` until daemon responds (up to 10s)
10. WebView loads dashboard → connects to daemon API

## Prerequisites

1. **Android SDK** (API 28+) with NDK 26.x
2. **Rust target**: `rustup target add aarch64-linux-android`
3. **JDK 17** (not newer — Gradle 8.x incompatibility with JDK 21+)
4. **llama.cpp** built for Android (see below)
5. **bun** (for dashboard build)

### Environment

```bash
export ANDROID_HOME=$HOME/Library/Android/sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/26.1.10909125
export JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home
```

## Building

### Build llama.cpp for Android (one-time)

```bash
git clone --depth 1 https://github.com/ggml-org/llama.cpp.git ~/Documents/SignetAI/llama.cpp
cd ~/Documents/SignetAI/llama.cpp
mkdir build-android-static && cd build-android-static
cmake .. \
  -DCMAKE_TOOLCHAIN_FILE=$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake \
  -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-28 \
  -DANDROID_STL=c++_static -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_BUILD_SERVER=ON -DGGML_OPENMP=OFF -DGGML_CUDA=OFF \
  -DGGML_METAL=OFF -DGGML_BLAS=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build . --target llama-server -j$(sysctl -n hw.ncpu)
$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-strip \
  bin/llama-server -o bin/llama-server-stripped
```

### One-command APK build

```bash
scripts/build-android.sh
```

This handles: copying dashboard, downloading GGUF model, staging llama-server + daemon + model in assets, building Rust .so, and assembling APK.

### Release build

```bash
RELEASE=--release scripts/build-android.sh
```

### Install & Run

```bash
APK=$(find src-tauri/gen/android/app/build/outputs/apk/arm64 -name "*.apk" | head -1)
adb install -r "$APK"
adb shell am start -n ai.signet.app/.MainActivity
```

## On-Device Inference

The daemon uses `llama-cpp` as the default provider on Android (via `#[cfg(target_os = "android")]` in config defaults):

| Task | Default | Model | Port |
|------|---------|-------|------|
| Embedding | llama-cpp | nomic-embed-text-v1.5 Q4_K_M | :8080 |
| Extraction | llama-cpp | *(needs GGUF in models/)* | :8080 |
| Synthesis | llama-cpp | *(needs GGUF in models/)* | :8080 |

To add an extraction/synthesis LLM, place a GGUF file in `files/.agents/models/` and update `agent.yaml`:

```yaml
memory:
  pipelineV2:
    extraction:
      provider: llama-cpp
      model: qwen3.5-0.6b
```

The llama-server loads the first `.gguf` file found in `models/`. For multi-model support, additional configuration is needed.

## Key Decisions

- **Sidecar llama-server** instead of llama-cpp-rs — simpler to update, decoupled from Rust FFI
- **`rustls-tls`** instead of `native-tls` — OpenSSL doesn't cross-compile to Android cleanly
- **APK `assets/`** for all binaries — files next to the .so in `lib/` aren't accessible at runtime
- **Kotlin extracts**, Rust spawns — separation of extraction (needs Android `assets/` API) and spawning
- **`File(filesDir, ".agents")`** instead of `getDir(".agents")` — avoids Android's `app_` prefix
- **JDK 17** — Gradle 8.x fails with JDK 21+ (class file version mismatch)
- **Q4_K_M quant** — good quality/size tradeoff for on-device embedding (~95MB vs ~270MB Q8)

## Project Structure

```
src-tauri/
  src/
    lib.rs              — Tauri app setup (Android-gated)
    commands.rs         — Tauri commands (daemon_port, daemon_status, data_dir)
    daemon.rs           — Daemon lifecycle coordinator
    platform/
      mod.rs            — DaemonManager trait + factory
      android.rs        — Android: app_data_dir(), llama-server + daemon spawn, health checks
      linux/macos/win   — Desktop stubs
  Cargo.toml            — reqwest with rustls-tls, android_logger gated to android
  tauri.conf.json       — minSdkVersion 28, CSP, frontendDist

src-tauri/gen/android/  (generated by tauri android init, gitignored)
  app/src/main/
    java/.../MainActivity.kt       — Extract binaries + model, create config, start service
    java/.../SignetDaemonService.kt — Foreground service with notification
    AndroidManifest.xml            — Permissions, intent filters, service declaration
    assets/
      signet-daemon                — Cross-compiled daemon binary
      llama-server                 — Cross-compiled llama.cpp server
      nomic-embed-text-v1.5.Q4_K_M.gguf — Embedding model

daemon-rs/              — Copy of upstream daemon, patched for Android
  crates/signet-pipeline/src/
    embedding.rs        — OllamaProvider, OpenAIProvider, LlamaCppProvider, NoopProvider
    provider.rs         — OllamaLlmProvider, AnthropicProvider, LlamaCppLlmProvider
  crates/signet-core/src/
    config.rs           — Platform-gated defaults: llama-cpp on Android, ollama on desktop

scripts/
  build-android.sh      — One-command APK build (downloads model, stages assets)
  copy-dashboard.sh     — Copies built dashboard from upstream signetai
```

## Device Paths

| What | Path |
|------|------|
| Data directory | `/data/data/ai.signet.app/files/.agents/` |
| Daemon binary | `files/.agents/bin/signet-daemon` |
| llama-server | `files/.agents/bin/llama-server` |
| Embedding model | `files/.agents/models/nomic-embed-text-v1.5.Q4_K_M.gguf` |
| Agent config | `files/.agents/agent.yaml` |
| Daemon logs | `files/.agents/.daemon/logs/daemon.log` |
| llama-server logs | `files/.agents/.daemon/logs/llama-server.log` |
| Memory storage | `files/.agents/memory/` |
