#!/usr/bin/env bash
# Merge split model chunks back into original files using cat
# Usage: ./merge_chunks.sh [model_dir]

set -euo pipefail

MODEL_DIR="${1:-$(dirname "$0")}"

echo "Merging chunks in $MODEL_DIR..."

# Find all unique base filenames (without .partXXX suffix)
for part_file in "$MODEL_DIR"/*.part000; do
    [[ -e "$part_file" ]] || continue

    # Extract base name: remove .part000 suffix
    base_name="${part_file%.part000}"
    output_file="$base_name"

    echo "Merging: $(basename "$output_file")"

    # Find all parts for this file in order
    parts=("$MODEL_DIR"/"$(basename "$base_name")".part*)
    # Sort numerically by part number
    IFS=$'\n' parts=($(sort -V <<<"${parts[*]}"))
    unset IFS

    # Merge using cat
    cat "${parts[@]}" > "$output_file"

    # Verify size
    original_size=0
    for p in "${parts[@]}"; do
        original_size=$((original_size + $(stat -c%s "$p")))
    done
    merged_size=$(stat -c%s "$output_file")

    if [[ $merged_size -eq $original_size ]]; then
        echo "  ✓ Size verified: $merged_size bytes"
    else
        echo "  ✗ Size mismatch! Expected: $original_size, Got: $merged_size"
        exit 1
    fi

    # Remove all chunk files
    rm "${parts[@]}"
    echo "  ✓ Removed ${#parts[@]} chunks"
done

echo "All chunks merged successfully. Only merged files remain."