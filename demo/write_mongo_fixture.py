#!/usr/bin/env python3
"""Write demo/db/mongo-dump fixture (mongodump directory layout)."""
from __future__ import annotations

import json
import os
import struct
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "demo", "db", "mongo-dump", "sample")
BSON = os.path.join(OUT, "users.bson")
META = os.path.join(OUT, "users.metadata.json")


def encode_bson(doc: dict) -> bytes:
    try:
        import bson  # type: ignore

        return bson.encode(doc)
    except ImportError:
        pass

    # Minimal encoder for flat string/int docs used in demo.
    def el(key: str, val) -> bytes:
        if isinstance(val, int):
            return b"\x10" + key.encode() + b"\x00" + struct.pack("<i", val)
        if isinstance(val, str):
            s = val.encode() + b"\x00"
            return b"\x02" + key.encode() + b"\x00" + struct.pack("<i", len(s)) + s
        raise TypeError(val)

    body = b"".join(el(k, v) for k, v in doc.items()) + b"\x00"
    size = len(body) + 4
    return struct.pack("<i", size) + body


def main() -> int:
    os.makedirs(OUT, exist_ok=True)
    docs = [
        {"_id": 1, "email": "a@example.com", "status": "ok"},
        {"_id": 2, "email": "b@example.com", "status": "failed"},
        {"_id": 3, "email": "c@example.com", "status": "ok"},
    ]
    with open(BSON, "wb") as f:
        for d in docs:
            f.write(encode_bson(d))
    meta = {
        "indexes": [{"v": 2, "key": {"_id": 1}, "name": "_id_", "ns": "sample.users"}],
        "collectionName": "users",
        "type": "collection",
    }
    with open(META, "w", encoding="utf-8") as f:
        json.dump(meta, f)
    print(f"mongo: {BSON}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
