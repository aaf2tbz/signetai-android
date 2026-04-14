# Signet Android

Native Android APK wrapping the Signet daemon and SvelteKit dashboard using Tauri 2. Fully self-contained — binaries and models are bundled in the APK and extracted on first launch. On-device inference via two llama.cpp sidecar instances for embeddings and extraction.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  APK                                                          │
│                                                               │
│  ┌──────────┐  ┌──────────────┐  ┌───────────┐ ┌──────────┐ │
│  │ Tauri 2  │  │ signet-daemon│  │llama-server│ │llama-    │ │
│  │ WebView  │──│ :3850 API    │──│ :8080      │ │server    │ │
│  │ Dashboard│  │ pipeline     │  │ embeddings │ │:8081     │ │
│  └──────────┘  └──────────────┘  └─────┬──────┘ └────┬─────┘ │
│                                        │              │       │
│                                  ┌─────┴────┐  ┌─────┴─────┐ │
│                                  │ nomic    │  │ Qwen2.5   │ │
│                                  │ embed    │  │ 1.5B      │ │
│                                  │ 95MB Q4  │  │ 897MB Q4  │ │
│                                  └──────────┘  └───────────┘ │
└──────────────────────────────────────────────────────────────┘
```

- **llama-server :8080** — `nomic-embed-text-v1.5` Q4_K_M (95MB) for vector embeddings
- **llama-server :8081** — `Qwen2.5-1.5B-Instruct` Q4_0_4_8 (897MB, ARM-optimized) for extraction/synthesis
- **signet-daemon :3850** — memory pipeline, API, SQLite + sqlite-vec storage
- **Tauri WebView** — full SvelteKit dashboard, connects to daemon API

## Flow on Device

1. App launches → `MainActivity.onCreate()`
2. Kotlin extracts binaries (`signet-daemon`, `llama-server`) from APK `assets/` to `files/.agents/bin/`
3. Kotlin extracts GGUF models to `files/.agents/models/`
4. Creates `files/.agents/agent.yaml` with `llama-cpp` providers
5. Starts foreground service
6. Rust spawns **llama-server :8080** with `--embedding` + nomic-embed model
7. Rust spawns **llama-server :8081** with Qwen2.5-1.5B model
8. Spawns **signet-daemon :3850** with `SIGNET_PATH` → `.agents/`
9. WebView loads dashboard → connects to daemon API

## Models

| Task | Model | Quant | Size | Port |
|------|-------|-------|------|------|
| Embedding | nomic-embed-text-v1.5 | Q4_K_M | 95MB | :8080 |
| Extraction | Qwen2.5-1.5B-Instruct | Q4_0_4_8 (ARM-optimized) | 897MB | :8081 |
| Synthesis | Qwen2.5-1.5B-Instruct | Q4_0_4_8 | 897MB | :8081 |

The `Q4_0_4_8` quantization uses ARM `i8mm` instructions for ~2x inference speedup on Snapdragon 8 Gen 1 (Galaxy S22). At 1.5B params, Qwen2.5 is capable of structured JSON extraction (entities, aspects, confidence scores) at ~15-20 tok/s on-device.

## Prerequisites

1. **Android SDK** (API 28+) with NDK 26.x
2. **Rust target**: `rustup target add aarch64-linux-android`
3. **JDK 17** (not newer — Gradle 8.x incompatibility with JDK 21+)
4. **llama.cpp** cross-compiled for Android (see below)
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

Downloads models, stages llama-server + daemon + models in assets, builds Rust .so, assembles APK.

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

## Key Decisions

- **Two llama-server instances** — one for embeddings (nomic, :8080), one for extraction (Qwen, :8081). llama.cpp loads one model per process.
- **Qwen2.5-1.5B-Instruct** — best size/quality tradeoff for structured extraction on mobile. 0.5B too dumb, 3B RAM-tight with both models loaded.
- **Q4_0_4_8 quantization** — ARM i8mm instruction set optimization, ~2x speedup on Snapdragon 8 Gen 1
- **Sidecar llama-server** instead of llama-cpp-rs — simpler to update, decoupled from Rust FFI
- **`rustls-tls`** instead of `native-tls` — OpenSSL doesn't cross-compile to Android cleanly
- **APK `assets/`** for all binaries — files next to the .so in `lib/` aren't accessible at runtime
- **Kotlin extracts**, Rust spawns — separation of extraction (needs Android `assets/` API) and spawning
- **JDK 17** — Gradle 8.x fails with JDK 21+ (class file version mismatch)

## Project Structure

```
src-tauri/
  src/
    lib.rs              — Tauri app setup (Android-gated)
    commands.rs         — Tauri commands (daemon_port, daemon_status, data_dir)
    daemon.rs           — Daemon lifecycle coordinator
    platform/
      mod.rs            — DaemonManager trait + factory
      android.rs        — Dual llama-server spawn, daemon spawn, health checks
      linux/macos/win   — Desktop stubs
  Cargo.toml            — reqwest with rustls-tls, android_logger gated to android
  tauri.conf.json       — minSdkVersion 28, CSP, frontendDist

src-tauri/gen/android/  (generated by tauri android init, gitignored)
  app/src/main/
    java/.../MainActivity.kt       — Extract binaries + models, create config, start service
    java/.../SignetDaemonService.kt — Foreground service with notification
    AndroidManifest.xml            — Permissions, intent filters, service declaration
    assets/
      signet-daemon                — Cross-compiled daemon (13MB)
      llama-server                 — Cross-compiled llama.cpp server (15MB)
      nomic-embed-text-v1.5.Q4_K_M.gguf — Embedding model (80MB)
      Qwen2.5-1.5B-Instruct-Q4_0_4_8.gguf — Extraction model (892MB)

daemon-rs/              — Copy of upstream daemon, patched for Android
  crates/signet-pipeline/src/
    embedding.rs        — OllamaProvider, OpenAIProvider, LlamaCppProvider
    provider.rs         — OllamaLlmProvider, AnthropicProvider, LlamaCppLlmProvider
  crates/signet-core/src/
    config.rs           — Platform-gated defaults: llama-cpp on Android, endpoints per port

scripts/
  build-android.sh      — One-command APK build (downloads models, stages assets)
  copy-dashboard.sh     — Copies built dashboard from upstream signetai
```

## Device Paths

| What | Path |
|------|------|
| Data directory | `/data/data/ai.signet.app/files/.agents/` |
| Daemon binary | `files/.agents/bin/signet-daemon` |
| llama-server | `files/.agents/bin/llama-server` |
| Embedding model | `files/.agents/models/nomic-embed-text-v1.5.Q4_K_M.gguf` |
| Extraction model | `files/.agents/models/Qwen2.5-1.5B-Instruct-Q4_0_4_8.gguf` |
| Agent config | `files/.agents/agent.yaml` |
| Daemon logs | `files/.agents/.daemon/logs/daemon.log` |
| llama-server embed logs | `files/.agents/.daemon/logs/llama-server-embed.log` |
| llama-server llm logs | `files/.agents/.daemon/logs/llama-server-llm.log` |
| Memory storage | `files/.agents/memory/` |
