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

echo "=== Signet Android Build ==="
echo "ANDROID_HOME: $ANDROID_HOME"
echo "ANDROID_NDK_HOME: $ANDROID_NDK_HOME"
echo "JAVA_HOME: $JAVA_HOME"
echo ""

echo "[1/4] Copying dashboard..."
"$REPO_ROOT/scripts/copy-dashboard.sh"

echo "[2/4] Building Rust library (aarch64-linux-android)..."
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

echo "[3/4] Assembling APK..."
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

echo "[4/4] Done!"
APK_DIR="$REPO_ROOT/src-tauri/gen/android/app/build/outputs/apk/arm64"
APK=$(find "$APK_DIR" -name "*.apk" | head -1)
if [ -n "$APK" ]; then
    SIZE=$(du -sh "$APK" | cut -f1)
    echo ""
    echo "APK: $APK ($SIZE)"
    echo ""
    echo "Install:  adb install -r $APK"
    echo "Run:      adb shell am start -n ai.signet.app/.MainActivity"
fi
