# Log demo fixtures

Samples for `omnicat log` — one file per detected format.

| File | Format | Try |
|------|--------|-----|
| `json.log` | JSON lines | `omnicat log demo/log/json.log --stats` |
| `logfmt.log` | logfmt (`key=value`) | `omnicat log demo/log/logfmt.log --errors` |
| `nginx.log` | nginx-style access | `omnicat log demo/log/nginx.log --http` |
| `tracing.log` | tracing / OTel fields | `omnicat log demo/log/tracing.log --trace trace-001` |
| `text.log` | plain text + level | `omnicat log demo/log/text.log --warnings` |

Compressed copies (run `./demo/generate.sh`):

| File | Codec |
|------|-------|
| `json.log.gz` | gzip |
| `json.log.zst` | zstd |
| `json.log.bz2` | bzip2 |
| `json.log.xz` | xz |

```bash
omnicat log demo/log/json.log.gz --stats
omnicat log demo/log/json.log --query 'top message limit 5'
omnicat log demo/log/json.log demo/log/text.log --errors
```

Verify: `./demo/smoke-log-db.sh`
