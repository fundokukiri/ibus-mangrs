#!/usr/bin/env python3
"""
Split model files > 100MB into chunks for GitHub upload.
Usage: python3 scripts/split_model.py [--chunk-size 100MB] [files...]
"""

import os
import sys
import argparse
from pathlib import Path

CHUNK_SIZE = 100 * 1024 * 1024  # 100MB
MANIFEST_NAME = ".model_manifest.json"


def split_file(path: Path, chunk_size: int) -> list:
    """Split a file into chunks. Returns list of chunk info."""
    size = path.stat().st_size
    if size <= chunk_size:
        return []

    chunks = []
    with open(path, "rb") as f:
        idx = 0
        while True:
            data = f.read(chunk_size)
            if not data:
                break
            chunk_name = f"{path.name}.part{idx:03d}"
            chunk_path = path.with_name(chunk_name)
            with open(chunk_path, "wb") as cf:
                cf.write(data)
            chunks.append({"name": chunk_name, "size": len(data)})
            idx += 1

    # Remove original
    path.unlink()
    return chunks


def write_manifest(model_dir: Path, manifest: dict):
    import json
    manifest_path = model_dir / MANIFEST_NAME
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)


def main():
    parser = argparse.ArgumentParser(description="Split model files > 100MB")
    parser.add_argument("--chunk-size", type=int, default=CHUNK_SIZE,
                        help="Chunk size in bytes (default: 100MB)")
    parser.add_argument("files", nargs="*", help="Files to split (default: all in data/model/)")
    args = parser.parse_args()

    model_dir = Path("data/model")
    if args.files:
        targets = [model_dir / f for f in args.files]
    else:
        targets = list(model_dir.glob("*.bin"))

    manifest = {"chunks": {}}
    any_split = False

    for path in targets:
        if not path.exists():
            print(f"SKIP missing: {path}")
            continue
        size = path.stat().st_size
        if size <= args.chunk_size:
            print(f"OK  {path.name} ({size/1024/1024:.1f} MB) - no split needed")
            continue

        print(f"SPLIT {path.name} ({size/1024/1024:.1f} MB) -> chunks of {args.chunk_size/1024/1024:.0f} MB")
        chunks = split_file(path, args.chunk_size)
        manifest["chunks"][path.name] = chunks
        any_split = True
        for c in chunks:
            print(f"  -> {c['name']} ({c['size']/1024/1024:.1f} MB)")

    if any_split:
        write_manifest(model_dir, manifest)
        print(f"\nManifest written: {model_dir / MANIFEST_NAME}")
    else:
        print("\nNo files needed splitting.")


if __name__ == "__main__":
    main()