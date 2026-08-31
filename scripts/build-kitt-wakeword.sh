#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 ]]; then
  echo "usage: $0 OUTPUT.rpw SAMPLE1.wav SAMPLE2.wav SAMPLE3.wav [more.wav ...]" >&2
  exit 2
fi

out="$1"
shift
mkdir -p "$(dirname "$out")"
exec rustpotter-cli build --model-name "kitt" --model-path "$out" "$@"
