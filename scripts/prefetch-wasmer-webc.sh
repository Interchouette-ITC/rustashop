#!/usr/bin/env bash
# Prefetch pinned Wasmer webc packages into RUSTASHOP_WASMER_CACHE (default .wasmer).
# Used by CI so workspace tests / coverage do not cold-download php-32 in parallel.
set -euo pipefail

ROOT="${RUSTASHOP_WASMER_CACHE:-.wasmer}"
DL="${ROOT}/downloads"
mkdir -p "${DL}"

prefetch() {
  local url="$1"
  local name="$2"
  local out="${DL}/${name}"
  if [[ -f "${out}" && -s "${out}" ]]; then
    echo "wasmer cache hit: ${name}"
    return 0
  fi
  echo "wasmer cache miss: fetching ${name}"
  curl -fsSL --retry 3 --retry-delay 2 \
    -H "Accept: application/webc" \
    -o "${out}.partial" \
    "${url}"
  mv "${out}.partial" "${out}"
  echo "wasmer cached: ${name} ($(wc -c < "${out}") bytes)"
}

prefetch "https://wasmer.io/python/python@0.1.0" "python-python-0.1.0.webc"
prefetch "https://wasmer.io/syrusakbary/quickjs" "syrusakbary-quickjs.webc"
prefetch "https://wasmer.io/php/php-32" "php-php-32.webc"
