#!/usr/bin/env python3
"""Generate a large MySQL-style dump for omnicat db stress testing."""
from __future__ import annotations

import argparse
import os
import sys

CREATE = """\
CREATE TABLE `events` (
  `id` BIGINT NOT NULL,
  `user_id` INT NOT NULL,
  `status` VARCHAR(32) NOT NULL,
  `payload` VARCHAR(256) NOT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_status` (`status`)
);
"""


def row_values(rid: int) -> str:
    status = "failed" if rid % 10_000 == 0 else "ok"
    uid = rid % 50_000
    pad = f"payload-{rid:012d}-{'x' * 180}"
    return f"({rid},{uid},'{status}','{pad}')"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("out", help="output .sql path")
    ap.add_argument(
        "--target-gib",
        type=float,
        default=5.0,
        help="approximate uncompressed size (default 5 GiB)",
    )
    ap.add_argument("--batch", type=int, default=5000, help="rows per INSERT")
    args = ap.parse_args()

    target = int(args.target_gib * (1024**3))
    batch = max(1, args.batch)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)) or ".", exist_ok=True)

    written = 0
    rid = 1
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        f.write("-- omnicat large test dump\n")
        f.write(CREATE)
        f.write("\n")
        written = f.tell()

        while written < target:
            chunk = ["INSERT INTO `events` VALUES "]
            chunk.append(row_values(rid))
            rid += 1
            for _ in range(1, batch):
                if written >= target:
                    break
                chunk.append(",")
                chunk.append(row_values(rid))
                rid += 1
            chunk.append(";\n")
            line = "".join(chunk)
            f.write(line)
            written += len(line.encode("utf-8"))
            if rid % 500_000 == 0:
                gib = written / (1024**3)
                print(f"  {gib:.2f} GiB, {rid:,} rows", file=sys.stderr, flush=True)

    print(f"wrote {args.out}: {written / (1024**3):.2f} GiB, {rid - 1:,} rows", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
