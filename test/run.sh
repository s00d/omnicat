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

echo
printf 'passed: %d, failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
