#!/usr/bin/env bash
# Split model files into 10MB chunks using split command
# Usage: ./split_10mb.sh [model_dir]

set -euo pipefail

MODEL_DIR="${1:-$(dirname "$0")}"
CHUNK_SIZE="7M"

echo "Splitting files in $MODEL_DIR into ${CHUNK_SIZE} chunks..."

for file in "$MODEL_DIR"/*.bin; do
    # Skip already split files (those ending in .part*)
    [[ "$file" == *.part* ]] && continue

    filename=$(basename "$file")
    filesize=$(stat -c%s "$file")
    sizemb=$((filesize / 1024 / 1024))

    echo "Processing: $filename (${sizemb} MB)"

    # Split into 10MB chunks with numeric suffixes
    split -b "$CHUNK_SIZE" -d -a 3 "$file" "$MODEL_DIR/${filename}.part"

    # Count chunks created
    chunk_count=$(ls "$MODEL_DIR/${filename}.part"* 2>/dev/null | wc -l)
    echo "  -> Created $chunk_count chunks"

    # Verify total size matches
    total_chunks_size=0
    for chunk in "$MODEL_DIR/${filename}.part"*; do
        total_chunks_size=$((total_chunks_size + $(stat -c%s "$chunk")))
    done

    if [[ $total_chunks_size -eq $filesize ]]; then
        echo "  ✓ Size verified: $total_chunks_size bytes"
        rm "$file"
        echo "  ✓ Removed original: $filename"
    else
        echo "  ✗ Size mismatch! Original: $filesize, Chunks: $total_chunks_size"
        exit 1
    fi
done

echo "All files split successfully. Original .bin files removed."