#!/usr/bin/env bash
# Fails if any tracked *.rs file does not carry the SPDX license identifier
# as the first line of the file.
set -euo pipefail

expected='// SPDX-License-Identifier: MIT OR Apache-2.0'
missing=()

while IFS= read -r -d '' file; do
    first_line="$(head -n 1 "$file")"
    if [[ "$first_line" != "$expected" ]]; then
        missing+=("$file")
    fi
done < <(git ls-files -z '*.rs')

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "Missing SPDX license header (expected: $expected):" >&2
    for file in "${missing[@]}"; do
        echo "  $file" >&2
    done
    exit 1
fi

echo "All *.rs files carry the SPDX license header."
