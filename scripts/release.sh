#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  ./scripts/release.sh check <version>
  ./scripts/release.sh candidate <version>

Commands:
  check      Verify that the checked-out main commit is ready to be tagged.
  candidate  Run check, create or validate the local uw-v<version> tag, verify the
             release binary, and push only that tag to origin.
EOF
}

fail() {
    echo "error: $*" >&2
    exit 1
}

require_release_version() {
    local version="$1"

    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || \
        fail "version must be release semver (for example: 0.4.0)"
}

torii_base_version() {
    awk '
        /^\[workspace\.package\]$/ { in_workspace_package = 1; next }
        /^\[/ { in_workspace_package = 0 }
        in_workspace_package && /^version = / {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' Cargo.toml
}

require_clean_worktree() {
    [[ -z "$(git status --porcelain)" ]] || fail "working tree is not clean"
}

require_main_tip() {
    local branch head main_tip

    branch="$(git branch --show-current)"
    [[ "$branch" == "main" ]] || fail "release commands must run from local main, not ${branch:-detached HEAD}"

    git fetch --quiet origin main
    head="$(git rev-parse HEAD)"
    main_tip="$(git rev-parse origin/main)"
    [[ "$head" == "$main_tip" ]] || fail "local main does not equal origin/main"
}

local_tag_state() {
    local tag="$1"
    local tag_object tag_commit

    if ! git show-ref --verify --quiet "refs/tags/$tag"; then
        echo "absent"
        return
    fi

    tag_object="$(git cat-file -t "$tag")"
    [[ "$tag_object" == "tag" ]] || fail "$tag must be an annotated tag"

    tag_commit="$(git rev-parse "${tag}^{commit}")"
    [[ "$tag_commit" == "$(git rev-parse HEAD)" ]] || fail "$tag does not point at HEAD"
    echo "present"
}

require_remote_tag_absent() {
    local tag="$1"

    [[ -z "$(git ls-remote --tags origin "refs/tags/$tag")" ]] || \
        fail "$tag already exists on origin"
}

check() {
    local version="$1"
    local tag="uw-v${version}"
    local base_version tag_state

    require_release_version "$version"
    require_clean_worktree
    require_main_tip
    require_remote_tag_absent "$tag"
    tag_state="$(local_tag_state "$tag")"
    base_version="$(torii_base_version)"
    [[ -n "$base_version" ]] || fail "could not read workspace Cargo version"

    echo "release check passed"
    echo "  commit: $(git rev-parse --short HEAD)"
    echo "  tag: $tag ($tag_state locally, absent on origin)"
    echo "  torii base: v$base_version"
}

verify_release_binary() {
    local version="$1"
    local base_version actual expected

    base_version="$(torii_base_version)"
    cargo build --release --bin torii
    actual="$(target/release/torii --version)"
    expected="torii ${version}-uw (base torii v${base_version}, "

    [[ "$actual" == "$expected"* ]] || fail "unexpected binary version: $actual"
    [[ "$actual" != *unknown* ]] || fail "incomplete binary version: $actual"
    echo "verified: $actual"
}

candidate() {
    local version="$1"
    local tag="uw-v${version}"
    local tag_state

    check "$version"
    tag_state="$(local_tag_state "$tag")"
    if [[ "$tag_state" == "absent" ]]; then
        git tag -a "$tag" -m "Underware Torii ${version}"
    fi

    verify_release_binary "$version"

    # Recheck after the local build so a concurrent main update cannot receive this tag.
    require_main_tip
    require_remote_tag_absent "$tag"
    git push origin "refs/tags/$tag"
    echo "pushed release candidate tag $tag"
}

[[ $# -eq 2 ]] || {
    usage
    exit 2
}

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || fail "not inside a Git repository"
cd "$repo_root"

case "$1" in
    check)
        check "$2"
        ;;
    candidate)
        candidate "$2"
        ;;
    *)
        usage
        exit 2
        ;;
esac
