#!/usr/bin/env bash
# Export plain draw.io sources to ignored SVG previews, initially and on changes.
set -euo pipefail

repository_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "${repository_root}"

once=false
if [[ "${1:-}" == --once && "$#" == 1 ]]; then
  once=true
elif [[ "$#" != 0 ]]; then
  printf 'Usage: %s [--once]\n' "$0" >&2
  exit 2
fi

declare -A fingerprints=()
declare -A outcomes=()
temporary_directory=''
cleanup() {
  if [[ -n "${temporary_directory}" ]]; then
    rm -rf -- "${temporary_directory}"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export_diagram() {
  local path="$1" output="${1}.svg"
  # Keep the snapshot and output beside the source so replacement is atomic.
  if ! temporary_directory="$(mktemp -d -- "$(dirname -- "${path}")/.drawio-export.XXXXXX")"; then
    return 1
  fi
  if cp -- "${path}" "${temporary_directory}/source.drawio" \
    && ./t drawio --export --format svg --timeout 60 \
      --output "${temporary_directory}/output.svg" "${temporary_directory}/source.drawio" \
    && [[ -s "${temporary_directory}/output.svg" ]]; then
    if [[ -f "${path}" && ! -L "${output}" ]] \
      && mv -fT -- "${temporary_directory}/output.svg" "${output}"; then
      printf 'Generated %s\n' "${output}"
    else
      printf 'Could not replace output or source disappeared: %s\n' "${path}" >&2
      cleanup
      temporary_directory=''
      return 1
    fi
  else
    printf 'Export failed; preserved previous SVG for %s\n' "${path}" >&2
    cleanup
    temporary_directory=''
    return 1
  fi
  cleanup
  temporary_directory=''
}

scan() {
  local path fingerprint failed=0
  local -A seen=()
  while IFS= read -r -d '' path; do
    seen["${path}"]=yes
    # Hash bytes, not timestamps: atomic saves and same-size edits are detected.
    if ! fingerprint="$(sha256sum < "${path}")"; then
      failed=1
      continue
    fi
    if [[ "${once}" == true || "${fingerprints[${path}]:-}" != "${fingerprint}" \
      || ( ! -f "${path}.svg" && "${outcomes[${path}]:-}" == ok ) ]]; then
      fingerprints["${path}"]="${fingerprint}"
      if export_diagram "${path}"; then
        outcomes["${path}"]=ok
      else
        outcomes["${path}"]=failed
        failed=1
      fi
    fi
  done < <(find . \( -type d \( -name .git -o -name .cache -o -name target \
      -o -name .nao -o -name .tool-tool -o -name '.drawio-export.*' \) -prune \) \
      -o \( -type f -name '*.drawio' -print0 \))

  for path in "${!fingerprints[@]}"; do
    if [[ -z "${seen[${path}]:-}" ]]; then
      # Only remove previews whose source was observed during this session.
      if [[ ! -L "${path}.svg" ]]; then
        rm -f -- "${path}.svg"
      fi
      unset 'fingerprints[$path]' 'outcomes[$path]'
      printf 'Removed preview for deleted source %s\n' "${path}"
    fi
  done
  return "${failed}"
}

if [[ "${once}" == true ]]; then
  scan
else
  printf 'Watching .drawio sources; press Ctrl-C to stop.\n'
  while true; do
    # A failed export is retried when its source changes, avoiding repeated
    # desktop launches for an unchanged invalid diagram or absent display.
    scan || true
    sleep 1
  done
fi
