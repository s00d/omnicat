#!/usr/bin/env bash
# Regenerate binary demo fixtures under demo/files/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FILES="$ROOT/demo/files"
LARGE_PAGES="${LARGE_PAGES:-100}"
mkdir -p "$FILES"

python3 "$ROOT/demo/generate.py" "$FILES" "$LARGE_PAGES"

# SQLite (readable table preview)
rm -f "$FILES/sample.sqlite"
sqlite3 "$FILES/sample.sqlite" <<'SQL'
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  role TEXT
);
INSERT INTO users VALUES (1, 'Ada', 'admin');
INSERT INTO users VALUES (2, 'Bob', 'viewer');
INSERT INTO users VALUES (3, 'Chen', 'editor');
SQL

# Archives
rm -f "$FILES/sample.zip" "$FILES/sample.tar" "$FILES/sample.tar.gz"
(
  cd "$FILES"
  echo 'inner payload for zip demo' > archive-inner.txt
  zip -q sample.zip archive-inner.txt
  # Avoid macOS tar xattr/AppleDouble junk in demo archives
  COPYFILE_DISABLE=1 tar --no-xattrs -cf sample.tar archive-inner.txt 2>/dev/null \
    || COPYFILE_DISABLE=1 tar -cf sample.tar archive-inner.txt
  COPYFILE_DISABLE=1 tar --no-xattrs -czf sample.tar.gz archive-inner.txt 2>/dev/null \
    || COPYFILE_DISABLE=1 tar -czf sample.tar.gz archive-inner.txt
  rm -f archive-inner.txt
)

# Font: copy a small system TTF when available
FONT_SRC=""
for candidate in \
  "/System/Library/Fonts/Supplemental/Andale Mono.ttf" \
  "/System/Library/Fonts/Supplemental/Courier New.ttf" \
  "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"; do
  if [[ -f "$candidate" ]]; then
    FONT_SRC="$candidate"
    break
  fi
done
if [[ -n "$FONT_SRC" ]]; then
  cp "$FONT_SRC" "$FILES/sample.ttf"
else
  echo "warn: no system TTF found; skip sample.ttf" >&2
fi

# Compressed audio formats from demo WAV (optional; needs ffmpeg)
if command -v ffmpeg >/dev/null 2>&1; then
  ffmpeg -y -loglevel error -i "$FILES/sample.wav" -codec:a libmp3lame -qscale:a 4 "$FILES/sample.mp3" \
    || echo "warn: mp3 encode failed" >&2
  ffmpeg -y -loglevel error -i "$FILES/sample.wav" -codec:a flac "$FILES/sample.flac" \
    || echo "warn: flac encode failed" >&2
  if ffmpeg -y -loglevel error -i "$FILES/sample.wav" -codec:a libvorbis -qscale:a 4 "$FILES/sample.ogg" 2>/dev/null; then
    :
  elif ffmpeg -y -loglevel error -i "$FILES/sample.wav" -codec:a libopus -b:a 96k "$FILES/sample.ogg" 2>/dev/null; then
    :
  else
    echo "warn: ogg encode failed" >&2
  fi
  echo "audio: sample.mp3 sample.flac sample.ogg (as available)"
else
  echo "warn: ffmpeg not found; skip mp3/flac/ogg (install ffmpeg for full audio demo set)" >&2
fi

# MOBI/AZW from EPUB (optional; needs calibre ebook-convert)
if command -v ebook-convert >/dev/null 2>&1; then
  ebook-convert "$FILES/sample.epub" "$FILES/sample.mobi" \
    --title "omnicat MOBI demo" --authors "Demo Author" 2>/dev/null \
    || echo "warn: mobi encode failed" >&2
  if [[ -f "$FILES/sample.mobi" ]]; then
    cp "$FILES/sample.mobi" "$FILES/sample.azw3"
    echo "ebook: sample.mobi sample.azw3"
  fi
  if [[ -f "$FILES/sample-large.epub" ]]; then
    ebook-convert "$FILES/sample-large.epub" "$FILES/sample-large.mobi" \
      --title "omnicat large book ($LARGE_PAGES pages)" --authors "Demo Author" \
      2>/dev/null || echo "warn: large mobi encode failed" >&2
    if [[ -f "$FILES/sample-large.mobi" ]]; then
      ls -lh "$FILES/sample-large.mobi" | awk '{print "ebook: sample-large.mobi (" $5 ")"}'
    fi
  fi
else
  echo "warn: ebook-convert not found; skip mobi/azw3 (install calibre)" >&2
fi

echo "Demo fixtures ready in $FILES"
