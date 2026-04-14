#!/usr/bin/env bash
set -euo pipefail

SIGNETAI="${SIGNETAI_ROOT:-$HOME/Documents/SignetAI/signetai}"
DASHBOARD_SRC="$SIGNETAI/packages/cli/dashboard"
FRONTEND_DST="$(dirname "$0")/../src-tauri/frontend"

if [ ! -f "$DASHBOARD_SRC/build/index.html" ]; then
    echo "Dashboard not built. Building..."
    cd "$DASHBOARD_SRC"
    bun install
    bun run build
fi

echo "Copying dashboard build to frontend..."
rm -rf "$FRONTEND_DST"/*
cp -r "$DASHBOARD_SRC/build/"* "$FRONTEND_DST/"

SIZE=$(du -sh "$FRONTEND_DST" | cut -f1)
echo "Dashboard copied ($SIZE)"
