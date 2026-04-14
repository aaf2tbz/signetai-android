#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/26.1.10909125}"
export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home}"

NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
export PATH="$NDK_BIN:$PATH"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$NDK_BIN/aarch64-linux-android28-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-Clink-arg=-landroid -Clink-arg=-llog -Clink-arg=-lOpenSLES"

EMBED_MODEL_URL="https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q4_K_M.gguf"
EMBED_MODEL_FILE="nomic-embed-text-v1.5.Q4_K_M.gguf"
LLM_MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/Qwen2.5-1.5B-Instruct-Q4_0_4_8.gguf"
LLM_MODEL_FILE="Qwen2.5-1.5B-Instruct-Q4_0_4_8.gguf"
LLAMA_CPP_DIR="${LLAMA_CPP_DIR:-$HOME/Documents/SignetAI/llama.cpp}"

echo "=== Signet Android Build ==="
echo "ANDROID_HOME: $ANDROID_HOME"
echo "ANDROID_NDK_HOME: $ANDROID_NDK_HOME"
echo "JAVA_HOME: $JAVA_HOME"
echo ""

echo "[1/9] Copying dashboard..."
"$REPO_ROOT/scripts/copy-dashboard.sh"

echo "[2/9] Downloading models..."
MODELS_DIR="$REPO_ROOT/models"
mkdir -p "$MODELS_DIR"
for MODEL_FILE in "$EMBED_MODEL_FILE" "$LLM_MODEL_FILE"; do
    if [ -f "$MODELS_DIR/$MODEL_FILE" ]; then
        echo "  $MODEL_FILE already downloaded ($(du -sh "$MODELS_DIR/$MODEL_FILE" | cut -f1))"
    else
        if [ "$MODEL_FILE" = "$EMBED_MODEL_FILE" ]; then
            URL="$EMBED_MODEL_URL"
        else
            URL="$LLM_MODEL_URL"
        fi
        curl -L --progress-bar -o "$MODELS_DIR/$MODEL_FILE" "$URL"
        echo "  Downloaded $MODEL_FILE ($(du -sh "$MODELS_DIR/$MODEL_FILE" | cut -f1))"
    fi
done

echo "[3/9] Staging llama-server..."
LLAMA_BIN="$LLAMA_CPP_DIR/build-android-static/bin/llama-server-stripped"
if [ ! -f "$LLAMA_BIN" ]; then
    echo "  ERROR: llama-server not found at $LLAMA_BIN"
    echo "  Build it first: cd $LLAMA_CPP_DIR && ./scripts/build-android.sh"
    exit 1
fi

ASSETS_DIR="$REPO_ROOT/src-tauri/gen/android/app/src/main/assets"
mkdir -p "$ASSETS_DIR"
cp "$LLAMA_BIN" "$ASSETS_DIR/llama-server"
echo "  -> assets/llama-server ($(du -sh "$ASSETS_DIR/llama-server" | cut -f1))"

echo "[4/9] Cross-compiling daemon (aarch64-linux-android)..."
DAEMON_BIN="$REPO_ROOT/daemon-rs/target/aarch64-linux-android/release/signet-daemon"
if [ ! -f "$DAEMON_BIN" ]; then
    DAEMON_BIN="$REPO_ROOT/daemon-rs/target/aarch64-linux-android/debug/signet-daemon"
fi

if [ ! -f "$DAEMON_BIN" ]; then
    echo "  Building daemon from daemon-rs/..."
    cargo build \
        --package signet-daemon \
        --manifest-path "$REPO_ROOT/daemon-rs/Cargo.toml" \
        --target aarch64-linux-android \
        ${RELEASE:+--release}
    if [ "${RELEASE:-}" = "--release" ]; then
        DAEMON_BIN="$REPO_ROOT/daemon-rs/target/aarch64-linux-android/release/signet-daemon"
    else
        DAEMON_BIN="$REPO_ROOT/daemon-rs/target/aarch64-linux-android/debug/signet-daemon"
    fi
fi

cp "$DAEMON_BIN" "$ASSETS_DIR/signet-daemon"
echo "  -> assets/signet-daemon ($(du -sh "$ASSETS_DIR/signet-daemon" | cut -f1))"

echo "[5/9] Staging GGUF models..."
cp "$MODELS_DIR/$EMBED_MODEL_FILE" "$ASSETS_DIR/"
echo "  -> $EMBED_MODEL_FILE ($(du -sh "$MODELS_DIR/$EMBED_MODEL_FILE" | cut -f1))"
cp "$MODELS_DIR/$LLM_MODEL_FILE" "$ASSETS_DIR/"
echo "  -> $LLM_MODEL_FILE ($(du -sh "$MODELS_DIR/$LLM_MODEL_FILE" | cut -f1))"

echo "[6/9] Building Rust library (aarch64-linux-android)..."
cargo build \
    --package signet-android \
    --manifest-path "$REPO_ROOT/src-tauri/Cargo.toml" \
    --target aarch64-linux-android \
    --features tauri/custom-protocol \
    --lib \
    ${RELEASE:+--release}

PROFILE="${RELEASE:-debug}"
LIB="src-tauri/target/aarch64-linux-android/$PROFILE/libsignet_android_lib.so"
JNI_DIR="$REPO_ROOT/src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$JNI_DIR"
ln -sf "$REPO_ROOT/$LIB" "$JNI_DIR/libsignet_android_lib.so"
echo "  -> $LIB"

echo "[7/9] Assembling APK..."
GRADLE_TASK="assembleArm64Debug"
if [ "${RELEASE:-}" = "--release" ]; then
    GRADLE_TASK="assembleArm64Release"
fi

"$REPO_ROOT/src-tauri/gen/android/gradlew" \
    --project-dir "$REPO_ROOT/src-tauri/gen/android" \
    ":app:$GRADLE_TASK" \
    -x :app:rustBuildArm64Debug \
    -x :app:rustBuildUniversalDebug \
    -x :app:rustBuildArm64Release \
    -x :app:rustBuildUniversalRelease

echo "[8/9] Done!"
APK_DIR="$REPO_ROOT/src-tauri/gen/android/app/build/outputs/apk/arm64"
APK=$(find "$APK_DIR" -name "*.apk" | head -1)
if [ -n "$APK" ]; then
    SIZE=$(du -sh "$APK" | cut -f1)
    echo ""
    echo "APK: $APK ($SIZE)"
    echo ""
    echo "Install:  adb install -r $APK"
    echo "Run:      adb shell am start -n ai.signet.app/.MainActivity"
    echo ""
    echo "Contents:"
    unzip -l "$APK" | grep -E "assets/" | awk '{print "  "$4" ("$1" bytes)"}'
fi
