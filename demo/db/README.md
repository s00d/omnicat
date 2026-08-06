# Database demo fixtures

Read-only sources for `omnicat db` (no live DB connections).

| Path | Kind | Try |
|------|------|-----|
| `sample.sql` | MySQL dump | `omnicat db demo/db/sample.sql --tables` |
| `sample.sql.gz` | MySQL dump (gzip) | `omnicat db demo/db/sample.sql.gz --schema` |
| `sample.sql.zst` | MySQL dump (zstd) | `omnicat db demo/db/sample.sql.zst --stats` |
| `sample.rdb` | Redis RDB | `omnicat db demo/db/sample.rdb --stats` |
| `sample.aof` | Redis AOF | `omnicat db demo/db/sample.aof --stats` |
| `mysql-datadir/` | MySQL datadir (metadata) | `omnicat db demo/db/mysql-datadir/` |
| `redis-aof-dir/` | Redis AOF directory | `omnicat db demo/db/redis-aof-dir/ --stats` |
| `sample.sqlite` | SQLite database | `omnicat db demo/db/sample.sqlite --query "SELECT * FROM users LIMIT 5"` |
| `mongo-dump/sample/` | MongoDB mongodump dir | `omnicat db demo/db/mongo-dump/sample --table users --query '{"status":"failed"}'` |

Query examples:

```bash
omnicat db demo/db/sample.sql \
  --query "SELECT status, COUNT(*) FROM users GROUP BY status"

omnicat db demo/db/sample.sql --query "SELECT email FROM users LIMIT 1" --print-query
# → INSERT INTO `users` (`email`) VALUES ('a@example.com');

omnicat db demo/db/mongo-dump/sample --table users --query '{"status":"failed"}' -Q
```

Generated artifacts (`sample.sql.gz`, `sample.sql.zst`, `sample.sqlite`, `mongo-dump/`, datadir trees) are created by `./demo/generate.sh` (Mongo fixture also via `demo/write_mongo_fixture.py`).

Verify: `./demo/smoke-log-db.sh`
