#!/bin/sh
set -eu
cd "$(dirname "$0")"
RUNE_REPO="${RUNE_REPO:?Set RUNE_REPO to a checkout of anomalyco/rune at the pinned revision}"
test -x "$RUNE_REPO/scripts/web-build.sh" || {
  echo "Missing Rune web-build.sh" >&2; exit 1;
}
"$RUNE_REPO/scripts/web-build.sh" kitt-rune-ui --out "$(pwd)/dist" --no-html
cp web/index.html dist/index.html
cp web/bridge.js dist/bridge.js
printf 'Serve locally with: node web/serve.mjs\n'
