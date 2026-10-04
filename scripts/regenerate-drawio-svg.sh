#!/usr/bin/env bash
# Regenerate staged SVGs through draw.io; leave partially staged working copies intact.
set -euo pipefail

repository_root="$(git rev-parse --show-toplevel)"
cd -- "${repository_root}"

temporary_directory="$(mktemp -d)"
trap 'rm -rf -- "${temporary_directory}"' EXIT

git diff --cached --name-only --diff-filter=ACMR -z > "${temporary_directory}/paths"
paths=()
outputs=()
update_working=()

while IFS= read -r -d '' path; do
  [[ "${path}" == *.drawio.svg ]] || continue

  git --literal-pathspecs ls-files --stage -z -- "${path}" > "${temporary_directory}/entry"
  IFS= read -r -d '' entry < "${temporary_directory}/entry"
  read -r mode object_id stage <<< "${entry%%$'\t'*}"
  if [[ "${stage}" != 0 || ( "${mode}" != 100644 && "${mode}" != 100755 ) ]]; then
    printf 'Expected a staged regular file: %s\n' "${path}" >&2
    exit 1
  fi

  number="${#paths[@]}"
  source="${temporary_directory}/${number}.drawio.svg"
  output="${temporary_directory}/${number}.svg"
  git cat-file blob "${object_id}" > "${source}"

  printf 'Regenerating staged %s\n' "${path}"
  ./t drawio --export --format svg --embed-diagram --timeout 60 \
    --output "${output}" "${source}"
  if [[ ! -s "${output}" ]]; then
    printf 'draw.io did not produce an SVG for %s\n' "${path}" >&2
    exit 1
  fi

  new_object_id="$(git hash-object -w -- "${output}")"
  printf '%s %s\t%s\0' "${mode}" "${new_object_id}" "${path}" >> "${temporary_directory}/index"

  paths+=("${path}")
  outputs+=("${output}")
  if [[ ! -L "${path}" && -f "${path}" ]] && cmp -s -- "${path}" "${source}"; then
    update_working+=(yes)
  else
    update_working+=(no)
  fi
done < "${temporary_directory}/paths"

[[ "${#paths[@]}" -gt 0 ]] || exit 0

# Apply the entire rendered batch only after every export has succeeded.
git update-index -z --index-info < "${temporary_directory}/index"
for index in "${!paths[@]}"; do
  if [[ "${update_working[index]}" == yes ]]; then
    cat -- "${outputs[index]}" > "${paths[index]}"
  else
    printf 'Preserved unstaged edits in %s\n' "${paths[index]}"
  fi
done
