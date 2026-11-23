#!/usr/bin/env python3
"""
Merge split model chunks back into original files.
Usage: python3 scripts/merge_model.py [--model-dir data/model] [files...]
"""

import os
import sys
import json
import argparse
from pathlib import Path

MANIFEST_NAME = ".model_manifest.json"


def merge_file(model_dir: Path, original_name: str, chunks: list) -> bool:
    """Merge chunks back into original file. Returns True on success."""
    # Verify all chunks exist
    for c in chunks:
        chunk_path = model_dir / c["name"]
        if not chunk_path.exists():
            print(f"ERROR: missing chunk {c['name']} for {original_name}")
            return False

    output_path = model_dir / original_name
    if output_path.exists():
        print(f"SKIP {original_name} already exists")
        return True

    print(f"MERGE {original_name} from {len(chunks)} chunks...")
    with open(output_path, "wb") as out:
        for c in chunks:
            chunk_path = model_dir / c["name"]
            with open(chunk_path, "rb") as f:
                out.write(f.read())
    print(f"  -> {output_path} ({output_path.stat().st_size/1024/1024:.1f} MB)")
    return True


def main():
    parser = argparse.ArgumentParser(description="Merge split model chunks")
    parser.add_argument("--model-dir", default="data/model", help="Model directory")
    parser.add_argument("files", nargs="*", help="Original files to merge (default: all in manifest)")
    args = parser.parse_args()

    model_dir = Path(args.model_dir)
    manifest_path = model_dir / MANIFEST_NAME

    if not manifest_path.exists():
        print(f"No manifest at {manifest_path} - nothing to merge")
        return

    with open(manifest_path) as f:
        manifest = json.load(f)

    targets = args.files if args.files else list(manifest.get("chunks", {}).keys())

    ok = True
    for name in targets:
        if name not in manifest.get("chunks", {}):
            print(f"WARNING: {name} not in manifest")
            continue
        if not merge_file(model_dir, name, manifest["chunks"][name]):
            ok = False

    if ok:
        print("\nAll merges completed successfully.")
    else:
        print("\nSome merges failed.")
        sys.exit(1)


if __name__ == "__main__":
    main()