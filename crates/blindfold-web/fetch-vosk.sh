#!/usr/bin/env bash
# Fetch the offline speech assets into dist/vosk so the app can serve them *same-origin*.
#
# Why same-origin: the browser fetches both the Vosk library and the ~41 MB model from
# JS, and neither the GitHub release asset nor a cross-origin CDN reliably sends the CORS
# headers a cross-origin fetch needs (the release asset sends none). Served from our own
# Pages origin there is no CORS to satisfy, and the whole thing works offline once cached.
#
# The assets are deliberately *not* committed (see .gitignore): the model is 41 MB and the
# library 5.8 MB. This script pulls them at build time instead — into a persistent cache
# (~/.cache/blindfold-vosk) so repeated builds download once — then copies them into dist.
#
# Set VOSK_SKIP=1 to skip entirely (CI's e2e stubs the recogniser and never needs the real
# assets, so it does not pay the download).
set -euo pipefail

if [ "${VOSK_SKIP:-}" = "1" ]; then
  echo "fetch-vosk: VOSK_SKIP=1, skipping."
  exit 0
fi

# Pinned versions — bump deliberately, in lockstep with the recogniser wiring.
VOSK_JS_URL="https://cdn.jsdelivr.net/npm/vosk-browser@0.0.8/dist/vosk.js"
MODEL_URL="https://github.com/kiblitz/blindtactics/releases/download/vosk-model-en-small/vosk-model-small-en-us-0.15.tar.gz"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CACHE="${HOME}/.cache/blindfold-vosk"
# Trunk runs this as a post_build hook *before* it applies the staged distribution to
# dist/, so writes must go to the staging dir it is about to apply — writing to the final
# dist/ would be discarded. TRUNK_STAGING_DIR is set only under Trunk; a manual run falls
# back to dist/ so `bash fetch-vosk.sh` on its own still populates a serving directory.
STAGE="${TRUNK_STAGING_DIR:-${HERE}/dist}"
# On Windows, Trunk hands a native `C:\...` path; convert it to the POSIX form Git Bash's
# mkdir/cp expect. A no-op (cygpath absent) on Linux/macOS, where the path is already POSIX.
if command -v cygpath >/dev/null 2>&1; then STAGE="$(cygpath -u "$STAGE")"; fi
DEST="${STAGE}/vosk"
mkdir -p "$CACHE" "$DEST"

fetch() {
  local url="$1" out="$2"
  if [ ! -s "$out" ]; then
    echo "fetch-vosk: downloading $(basename "$out")..."
    curl -fsSL -o "$out" "$url"
  fi
}

fetch "$VOSK_JS_URL" "${CACHE}/vosk.js"
fetch "$MODEL_URL" "${CACHE}/model.tar.gz"

cp "${CACHE}/vosk.js" "${DEST}/vosk.js"
cp "${CACHE}/model.tar.gz" "${DEST}/model.tar.gz"
echo "fetch-vosk: assets in place at ${DEST}"
