#!/usr/bin/env python3
"""Debug script to diagnose the 400 error on POST /api/project."""
import urllib.request
import urllib.error
import json
from pathlib import Path

BASE_DIR = Path(__file__).resolve().parent.parent
fixture_path = str((BASE_DIR / "fixtures" / "once_cell").resolve())

payload = {
    "root_path": fixture_path,
    "name": "once_cell-bench",
    "extensions": ["rs", "toml", "md"],
}

data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
    "http://127.0.0.1:9001/api/project",
    data=data,
    headers={"Content-Type": "application/json"},
    method="POST",
)

try:
    with urllib.request.urlopen(req) as resp:
        print(f"Status: {resp.status}")
        print(f"Response: {resp.read().decode('utf-8')}")
except urllib.error.HTTPError as e:
    print(f"Status: {e.code}")
    print(f"Response: {e.read().decode('utf-8')}")
except Exception as e:
    print(f"Error: {e}")