#!/usr/bin/env bash
set -u

ROOT="$(cd -P "$(dirname "$0")/.." && pwd)"
BIN="${OMNICAT_BIN:-$ROOT/target/release/omnicat}"

PASS=0
FAIL=0

GREEN=""; RED=""; RESET=""
if [ -t 1 ]; then GREEN=$'\033[32m'; RED=$'\033[31m'; RESET=$'\033[0m'; fi

ok()   { PASS=$((PASS + 1)); printf '%s  ok %s%s\n' "$GREEN" "$1" "$RESET"; }
bad()  { FAIL=$((FAIL + 1)); printf '%sNOT ok %s%s\n' "$RED" "$1" "$RESET"; printf '       %s\n' "$2"; }

assert_eq() {
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "expected [$2] got [$3]"; fi
}
assert_contains() {
  case "$2" in
    *"$3"*) ok "$1" ;;
    *) bad "$1" "[$2] does not contain [$3]" ;;
  esac
}

if [ ! -x "$BIN" ]; then
  echo "Building release binary..."
  (cd "$ROOT" && cargo build --release)
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

MD="$TMP/sample.md"
printf '# Title\n\nSENTINEL-MD-CONTENT\n' > "$MD"
PY="$TMP/sample.py"
printf 'print("SENTINEL-PY")\n' > "$PY"
TXT="$TMP/notes.txt"
printf 'plain text\nwith two lines\n' > "$TXT"
BINFILE="$TMP/unknown.bin"
printf '\x00\x01SENTINEL-BIN' > "$BINFILE"

echo "== meta =="
assert_contains "version" "$("$BIN" --version)" "omnicat "
assert_contains "help mentions preview" "$("$BIN" --help)" "--preview"
assert_contains "help mentions info" "$("$BIN" --help)" "--info"
assert_contains "init zsh emits wrapper" "$("$BIN" init zsh)" "command omnicat"
assert_contains "init unknown shell errors" "$("$BIN" init fish 2>&1)" "unsupported shell"

echo "== passthrough (no TTY) =="
assert_eq "single file piped equals cat" "$(command cat "$MD")" "$("$BIN" "$MD" | command cat)"
assert_eq "multi file equals cat" "$(command cat "$MD" "$TXT")" "$("$BIN" "$MD" "$TXT" | command cat)"
assert_eq "flag -n equals cat -n" "$(command cat -n "$MD")" "$("$BIN" -n "$MD" | command cat)"
assert_eq "missing file behaves like cat" "$(command cat "$TMP/nope.md" 2>&1)" "$("$BIN" "$TMP/nope.md" 2>&1)"

echo "== status =="
sout="$("$BIN" -status)"
assert_contains "status lists markdown" "$sout" "markdown"
assert_contains "status lists spreadsheet" "$sout" "spreadsheet"
assert_contains "status shows driver renderer" "$sout" "driver:markdown(+)"
assert_contains "status shows builtin column" "$sout" "BUILTIN"
assert_contains "status shows external column" "$sout" "EXTERNAL"
assert_contains "status shows gui settings" "$sout" "GUI SETTINGS"

echo "== preview availability =="
out="$(OMNICAT_NO_GUI=1 "$BIN" -status 2>&1)"
assert_contains "gui marked unavailable" "$out" "gui: unavailable"

echo "== unknown format passthrough (pipe) =="
out="$("$BIN" "$BINFILE" | command cat)"
assert_contains "binary passthrough" "$out" "SENTINEL-BIN"

echo "== directory preview (pipe passthrough) =="
DIR="$TMP/sampledir"
mkdir -p "$DIR/sub"
printf 'nested\n' > "$DIR/sub/file.txt"
out="$(OMNICAT_NO_GUI=1 "$BIN" "$DIR" 2>&1 | command cat || true)"
# piped stdout is not TTY — passthrough; directory render tested in cargo test

echo "== archive tree (unit covered) =="
ZIP="$TMP/tree.zip"
python3 -c "import zipfile; z=zipfile.ZipFile('$ZIP','w'); z.writestr('a/b.txt','x'); z.close()" 2>/dev/null || true

echo "== display config override =="
ALT="$TMP/display.yaml"
cat > "$ALT" <<'EOF'
terminal:
  code:
    line_numbers: false
    theme: InspiredGitHub
    style: plain
    tab_width: 4
EOF
sout2="$(OMNICAT_CONFIG="$ALT" "$BIN" -status)"
assert_contains "display override applied" "$sout2" "line_numbers: false"

echo "== inspect =="
info_out="$("$BIN" --info --json "$TXT")"
assert_contains "info json has type" "$info_out" '"report": "info"'
assert_contains "capabilities lists preview" "$("$BIN" --capabilities)" "preview"
CSV="$TMP/people.csv"
printf 'id,name,age\n1,Alice,30\n2,Bob,17\n' > "$CSV"
schema_out="$("$BIN" --schema "$CSV")"
assert_contains "schema shows age" "$schema_out" "age"
query_out="$("$BIN" --query 'age > 18' "$CSV")"
assert_contains "query filters Alice" "$query_out" "Alice"
find_out="$("$BIN" --find Alice "$CSV")"
assert_contains "find finds Alice" "$find_out" "Alice"
stats_out="$("$BIN" --stats "$TMP")"
assert_contains "stats has files" "$stats_out" "Files"
ZIP2="$TMP/virt.zip"
python3 -c "import zipfile; z=zipfile.ZipFile('$ZIP2','w'); z.writestr('inner/hi.txt','hello-virt'); z.close()"
virt_out="$("$BIN" --text "$ZIP2/inner/hi.txt")"
assert_contains "virtual path text" "$virt_out" "hello-virt"
assert_contains "status shows inspect" "$("$BIN" -status)" "INSPECT"

HASHFILE="$TMP/hashme.bin"
printf 'hash-payload' > "$HASHFILE"
hash_out="$("$BIN" --hash --json "$HASHFILE")"
assert_contains "hash json has blake3" "$hash_out" '"blake3"'
DUPDIR="$TMP/dups"
mkdir -p "$DUPDIR"
printf 'dup-content' > "$DUPDIR/one.txt"
printf 'dup-content' > "$DUPDIR/two.txt"
printf 'unique' > "$DUPDIR/uniq.txt"
dup_out="$("$BIN" --duplicates "$DUPDIR")"
assert_contains "duplicates finds copies" "$dup_out" "reclaimable"

JSONL="$TMP/app.jsonl"
printf '%s\n' '{"ts":"12:01","level":"INFO","service":"api","msg":"Started"}' '{"ts":"12:02","level":"ERROR","service":"db","msg":"Timeout"}' > "$JSONL"
jsonl_out="$("$BIN" --head 5 "$JSONL")"
assert_contains "jsonl log columns" "$jsonl_out" "TIME"
assert_contains "jsonl log message" "$jsonl_out" "Timeout"

DBA="$TMP/a.sqlite"
DBB="$TMP/b.sqlite"
python3 - <<PY
import sqlite3
c=sqlite3.connect("$DBA"); c.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT)"); c.commit(); c.close()
c=sqlite3.connect("$DBB"); c.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL)"); c.commit(); c.close()
PY
diff_out="$("$BIN" --diff "$DBA" "$DBB")"
assert_contains "sqlite schema diff" "$diff_out" "schema:"

# tar.zst virtual path (needs zstd CLI or python zstandard)
TARZST="$TMP/demo.tar.zst"
TARPLAIN="$TMP/demo.tar"
if command -v zstd >/dev/null 2>&1; then
  mkdir -p "$TMP/tardir"
  printf 'hello-zst' > "$TMP/tardir/inner.txt"
  (cd "$TMP" && tar -cf "$TARPLAIN" -C tardir inner.txt && zstd -q -f -o "$TARZST" "$TARPLAIN")
  zst_out="$("$BIN" --text "$TARZST/inner.txt")"
  assert_contains "tar.zst virtual text" "$zst_out" "hello-zst"
  zst_stats="$("$BIN" --stats "$TARZST")"
  assert_contains "tar.zst stats" "$zst_stats" "Entries"
else
  ok "tar.zst skipped (no zstd CLI)"
fi

LOGFILE="$TMP/app.log"
printf '%s\n' '{"level":"error","service":"api","msg":"fail"}' '{"level":"info","service":"api","msg":"ok"}' > "$LOGFILE"
log_stats="$("$BIN" log --stats "$LOGFILE")"
assert_contains "log stats messages" "$log_stats" "Messages"
log_err="$("$BIN" log --errors "$LOGFILE")"
assert_contains "log errors filter" "$log_err" "fail"

JSONL="$TMP/events.jsonl"
printf '%s\n' '{"level":"info","message":"ok"}' '{"level":"error","message":"Timeout"}' > "$JSONL"
find_jsonl="$("$BIN" --find 'level:error' "$JSONL")"
assert_contains "structured find jsonl" "$find_jsonl" "Timeout"

CTXLOG="$TMP/ctx.log"
printf '%s\n' '2026-08-06T12:42:15Z INFO before' '2026-08-06T12:42:17Z ERROR hit' '2026-08-06T12:42:19Z INFO after' > "$CTXLOG"
ctx_out="$("$BIN" log --around 12:42:17 --context 1 "$CTXLOG")"
assert_contains "log context around" "$ctx_out" "hit"
assert_contains "log context neighbor" "$ctx_out" "before"

DEMO_DB="$ROOT/demo/db"
if [ -f "$DEMO_DB/sample.sql" ]; then
  db_query="$("$BIN" db "$DEMO_DB/sample.sql" --query 'SELECT email FROM users LIMIT 1')"
  assert_contains "db mysql query" "$db_query" "a@example.com"
  db_schema="$("$BIN" db "$DEMO_DB/sample.sql" --schema)"
  assert_contains "db mysql schema" "$db_schema" "users"
fi
if [ -f "$DEMO_DB/sample.rdb" ]; then
  db_rdb="$("$BIN" db "$DEMO_DB/sample.rdb" --stats)"
  assert_contains "db redis rdb stats" "$db_rdb" "Keys"
fi
if [ -f "$DEMO_DB/sample.aof" ]; then
  db_aof="$("$BIN" db "$DEMO_DB/sample.aof" --stats)"
  assert_contains "db redis aof stats" "$db_aof" "SET"
fi

echo
printf 'passed: %d, failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
