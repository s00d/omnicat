#!/usr/bin/env bash
# Verify demo fixtures exist; optionally render in a PTY (see cargo test demo_fixtures).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FILES="$ROOT/demo/files"

missing=0
while read -r rel; do
  [[ -z "$rel" || "$rel" =~ ^# ]] && continue
  if [[ ! -e "$ROOT/demo/$rel" ]]; then
    echo "MISSING: demo/$rel"
    missing=$((missing + 1))
  fi
done <<'LIST'
files/sample.md
files/sample.rs
files/sample.py
files/sample.json
files/sample.yaml
files/sample.toml
files/sample.ini
files/sample.csv
files/sample.tsv
files/sample.txt
files/sample.rtf
files/sample.eml
files/sample.plist
files/sample.fb2
files/sample.ipynb
files/sample.png
files/sample.gif
files/sample.pdf
files/sample.wav
files/sample.zip
files/sample.tar
files/sample.tar.gz
files/sample.docx
files/sample.odt
files/sample.xlsx
files/sample.ods
files/sample.pptx
files/sample.odp
files/sample.epub
files/sample.doc
files/sample.bin
files/sample.sqlite
files/sample.ttf
dir-tree/README.txt
dir-tree/nested/deep.txt
LIST

if [[ "$missing" -gt 0 ]]; then
  echo
  echo "$missing path(s) missing — run: ./demo/generate.sh"
  exit 1
fi

echo "All demo paths present under demo/"

if [[ "${SMOKE_RENDER:-0}" == "1" ]]; then
  echo "Running demo_fixtures cargo test..."
  cargo test -q demo_fixtures --manifest-path "$ROOT/Cargo.toml"
fi
