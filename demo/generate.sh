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

# --- log + db demo fixtures (omnicat log / omnicat db) ---
LOG="$ROOT/demo/log"
DB="$ROOT/demo/db"
mkdir -p "$LOG" "$DB"

# Compressed log (all codecs supported by io/source)
if [[ -f "$LOG/json.log" ]]; then
  gzip -cn "$LOG/json.log" > "$LOG/json.log.gz"
  if command -v zstd >/dev/null 2>&1; then
    zstd -q -f "$LOG/json.log" -o "$LOG/json.log.zst"
  else
    echo "warn: zstd not found; skip demo/log/json.log.zst" >&2
  fi
  if command -v bzip2 >/dev/null 2>&1; then
    bzip2 -c "$LOG/json.log" > "$LOG/json.log.bz2"
  fi
  if command -v xz >/dev/null 2>&1; then
    xz -c "$LOG/json.log" > "$LOG/json.log.xz"
  fi
  echo "log: json.log + .gz/.zst/.bz2/.xz (as available)"
fi

# Compressed MySQL dump
if [[ -f "$DB/sample.sql" ]]; then
  gzip -cn "$DB/sample.sql" > "$DB/sample.sql.gz"
  if command -v zstd >/dev/null 2>&1; then
    zstd -q -f "$DB/sample.sql" -o "$DB/sample.sql.zst"
  else
    echo "warn: zstd not found; skip demo/db/sample.sql.zst" >&2
  fi
  echo "db: sample.sql.gz sample.sql.zst (as available)"
fi

# MySQL datadir layout (read-only metadata demo)
DATADIR="$DB/mysql-datadir"
mkdir -p "$DATADIR"
printf 'ibdata stub\n' > "$DATADIR/ibdata1"
truncate -s 4096 "$DATADIR/users.ibd" 2>/dev/null || dd if=/dev/zero of="$DATADIR/users.ibd" bs=4096 count=1 2>/dev/null
truncate -s 8192 "$DATADIR/orders.ibd" 2>/dev/null || dd if=/dev/zero of="$DATADIR/orders.ibd" bs=8192 count=1 2>/dev/null
echo "db: mysql-datadir/ (ibdata1 + *.ibd)"

# Redis AOF directory (appendonly.aof)
AOFDIR="$DB/redis-aof-dir"
mkdir -p "$AOFDIR"
if [[ -f "$DB/sample.aof" ]]; then
  cp "$DB/sample.aof" "$AOFDIR/appendonly.aof"
  echo "db: redis-aof-dir/appendonly.aof"
fi

# Redis RDB: use rdb crate test fixture (Redis 8 dumps break rdb 0.3)
RDB_FIXTURE=""
for candidate in \
  "$HOME/.cargo/registry/src/"*/rdb-0.3.0/tests/dumps/empty_database.rdb \
  "$ROOT/target/registry/src/"*/rdb-0.3.0/tests/dumps/empty_database.rdb; do
  if [[ -f "$candidate" ]]; then
    RDB_FIXTURE="$candidate"
    break
  fi
done
if [[ -n "$RDB_FIXTURE" ]]; then
  cp "$RDB_FIXTURE" "$DB/sample.rdb"
  echo "db: sample.rdb (empty_database.rdb from rdb crate)"
elif [[ ! -f "$DB/sample.rdb" ]]; then
  echo "warn: sample.rdb missing; run cargo fetch && ./demo/generate.sh" >&2
fi

echo "Run ./demo/smoke-log-db.sh to verify log + db fixtures"

# MongoDB mongodump directory fixture
python3 "$ROOT/demo/write_mongo_fixture.py" 2>/dev/null || echo "warn: mongo fixture script failed" >&2

# SQLite demo for omnicat db (same users as sample.sql)
rm -f "$DB/sample.sqlite"
sqlite3 "$DB/sample.sqlite" <<'SQL'
CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT, status TEXT);
INSERT INTO users VALUES (1, 'a@example.com', 'ok');
INSERT INTO users VALUES (2, 'b@example.com', 'failed');
INSERT INTO users VALUES (3, 'c@example.com', 'ok');
SQL
echo "db: sample.sqlite"
