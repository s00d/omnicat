#!/usr/bin/env bash
# Smoke-test demo/log and demo/db fixtures with omnicat.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${OMNICAT_BIN:-$ROOT/target/debug/omnicat}"
LOG="$ROOT/demo/log"
DB="$ROOT/demo/db"
PASS=0
FAIL=0

if [[ ! -x "$BIN" ]]; then
  echo "Building omnicat..."
  cargo build --quiet --manifest-path "$ROOT/Cargo.toml"
fi

ok() { echo "  ok $1"; PASS=$((PASS + 1)); }
bad() { echo "  FAIL $1"; FAIL=$((FAIL + 1)); }

assert_contains() {
  local name=$1 text=$2 needle=$3
  if [[ "$text" == *"$needle"* ]]; then ok "$name"; else bad "$name (missing: $needle)"; fi
}

assert_ok() {
  local name=$1
  shift
  if "$@" >/dev/null 2>&1; then ok "$name"; else bad "$name (exit $?)"; fi
}

echo "== demo/log =="
for f in json.log logfmt.log nginx.log tracing.log text.log; do
  [[ -f "$LOG/$f" ]] || { bad "missing $f"; continue; }
  out=$("$BIN" log --stats "$LOG/$f" 2>&1) && assert_contains "log stats $f" "$out" "Messages"
done

for f in json.log.gz json.log.zst json.log.bz2 json.log.xz; do
  [[ -f "$LOG/$f" ]] || continue
  out=$("$BIN" log --stats "$LOG/$f" 2>&1) && assert_contains "log compressed $f" "$out" "Messages"
done

err_out=$("$BIN" log --errors "$LOG/json.log" 2>&1) || true
assert_contains "log json errors" "$err_out" "timeout"

trace_out=$("$BIN" log --trace trace-001 "$LOG/tracing.log" 2>&1) || true
assert_contains "log tracing filter" "$trace_out" "cart"

echo
echo "== demo/db =="
assert_ok "db sql tables" "$BIN" db "$DB/sample.sql" --tables
assert_ok "db sql schema" "$BIN" db "$DB/sample.sql" --schema

if [[ -f "$DB/sample.sql.gz" ]]; then
  q=$("$BIN" db "$DB/sample.sql.gz" --query 'SELECT COUNT(*) AS n FROM users' 2>&1) || true
  assert_contains "db sql.gz query" "$q" "3"
fi
if [[ -f "$DB/sample.sql.zst" ]]; then
  assert_ok "db sql.zst schema" "$BIN" db "$DB/sample.sql.zst" --schema
fi

if [[ -f "$DB/sample.rdb" ]]; then
  rdb=$("$BIN" db "$DB/sample.rdb" --stats 2>&1) || true
  assert_contains "db rdb stats" "$rdb" "Redis RDB"
fi

aof=$("$BIN" db "$DB/sample.aof" --stats 2>&1) || true
assert_contains "db aof stats" "$aof" "SET"

if [[ -d "$DB/mysql-datadir" ]]; then
  dat=$("$BIN" db "$DB/mysql-datadir" 2>&1) || true
  assert_contains "db mysql datadir" "$dat" "WARNING"
  assert_contains "db mysql datadir ibd" "$dat" ".ibd"
fi

if [[ -d "$DB/redis-aof-dir" ]]; then
  dir=$("$BIN" db "$DB/redis-aof-dir" --stats 2>&1) || true
  assert_contains "db redis aof dir" "$dir" "Commands"
fi

q=$("$BIN" db "$DB/sample.sql" --query "SELECT email FROM users WHERE status = 'failed' LIMIT 1" 2>&1) || true
assert_contains "db sql query" "$q" "b@example.com"

if [[ -f "$DB/sample.sqlite" ]]; then
  sq=$("$BIN" db "$DB/sample.sqlite" --query "SELECT email FROM users WHERE status = 'failed' LIMIT 1" 2>&1) || true
  assert_contains "db sqlite query" "$sq" "b@example.com"
  assert_ok "db sqlite tables" "$BIN" db "$DB/sample.sqlite" --tables
fi

if [[ -d "$DB/mongo-dump/sample" ]]; then
  mq=$("$BIN" db "$DB/mongo-dump/sample" --table users --query '{"status":"failed"}' 2>&1) || true
  assert_contains "db mongo query" "$mq" "b@example.com"
  assert_ok "db mongo stats" "$BIN" db "$DB/mongo-dump/sample" --stats
fi

pq=$("$BIN" db "$DB/sample.sql" --query "SELECT email FROM users LIMIT 1" --print-query 2>&1) || true
assert_contains "db print-query sql" "$pq" "INSERT INTO"
assert_contains "db print-query value" "$pq" "a@example.com"

echo
printf 'passed: %d, failed: %d\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
