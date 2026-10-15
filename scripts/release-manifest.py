#!/usr/bin/env python3
"""Generate a manual-download manifest and checksums; never publishes."""
import argparse
import hashlib
import json
import pathlib
import re

parser = argparse.ArgumentParser()
parser.add_argument("version")
parser.add_argument("assets", type=pathlib.Path)
args = parser.parse_args()
if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", args.version):
    parser.error("version must be a stable major.minor.patch")
if not args.assets.is_dir():
    parser.error("assets directory must exist")
files = sorted(p for p in args.assets.iterdir() if p.is_file() and p.name not in {"latest.json", "SHA256SUMS"})
if not files:
    parser.error("no release assets; build and validate packages first")
checksums = []
for path in files:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    checksums.append(f"{digest.hexdigest()}  {path.name}\n")
(args.assets / "SHA256SUMS").write_text("".join(checksums))
(args.assets / "latest.json").write_text(json.dumps({"version": args.version, "notes": "Manual download from the versioned Release page.", "platforms": {}}, indent=2) + "\n")
