#!/usr/bin/env bash

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

missing_tools=()
for tool in actionlint shellcheck; do
    if ! command -v "$tool" >/dev/null; then
        missing_tools+=("$tool")
    fi
done

if ((${#missing_tools[@]})); then
    printf 'error: missing CI policy tool(s): %s\n' "${missing_tools[*]}" >&2
    printf 'see docs/contributor/testing.md for installation instructions\n' >&2
    exit 1
fi

shell_files=(
    .githooks/pre-commit
    bin/setup-githooks
    scripts/*.sh
)

bash -n "${shell_files[@]}"
shellcheck --severity=warning "${shell_files[@]}"

export SHELLCHECK_OPTS="--severity=warning"
actionlint

echo 'CI policy checks passed.'
