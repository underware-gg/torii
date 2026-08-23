#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  ./scripts/release.sh check <version>
  ./scripts/release.sh candidate <version>
  ./scripts/release.sh validate-version <version>
  ./scripts/release.sh validate-version-order <version>
  ./scripts/release.sh verify-binary <version> <binary>
  ./scripts/release.sh verify-settings

Commands:
  check      Verify that the checked-out main commit is ready to be tagged.
  candidate  Run check, create or validate the local uw-v<version> tag, verify the
             release binary, and push only that tag to origin.
  validate-version
             Verify that a version is canonical Underware release semver.
  validate-version-order
             Verify that a version is not older than any published Underware release.
  verify-binary
             Verify a built torii binary carries the expected Underware release identity.
  verify-settings
             Verify the GitHub release environment and repository protections are active.
EOF
}

fail() {
    echo "error: $*" >&2
    exit 1
}

require_release_version() {
    local version="$1"

    [[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || \
        fail "version must be release semver (for example: 0.4.0)"
}

torii_base_version() {
    local package_id

    package_id="$(cargo pkgid --package torii)" || fail "could not read Torii package version"
    printf '%s\n' "${package_id##*#}"
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
    local tag="$1" remote_tag

    remote_tag="$(git ls-remote --tags origin "refs/tags/$tag")" || \
        fail "could not query origin for $tag"
    [[ -z "$remote_tag" ]] || fail "$tag already exists on origin"
}

require_newer_than_published_release() {
    local version="$1" remote_tags tag tag_version
    local candidate_major candidate_minor candidate_patch published_major published_minor published_patch

    remote_tags="$(git ls-remote --refs --tags origin 'refs/tags/uw-v*')" || \
        fail "could not query origin release tags"
    while IFS=$'\t' read -r _ tag; do
        [[ -n "$tag" ]] || continue
        tag="${tag#refs/tags/uw-v}"
        tag_version="$tag"
        require_release_version "$tag_version"

        IFS=. read -r candidate_major candidate_minor candidate_patch <<<"$version"
        IFS=. read -r published_major published_minor published_patch <<<"$tag_version"
        if ((10#$candidate_major < 10#$published_major ||
            (10#$candidate_major == 10#$published_major && 10#$candidate_minor < 10#$published_minor) ||
            (10#$candidate_major == 10#$published_major && 10#$candidate_minor == 10#$published_minor && 10#$candidate_patch < 10#$published_patch))); then
            fail "version $version must be newer than published Underware release $tag_version"
        fi
    done <<<"$remote_tags"
}

require_release_settings() {
    local origin_url repository reviewers self_review admin_bypass main_reviews main_checks main_admins force_pushes deletions
    local tag_ruleset tag_rules tag_bypasses

    command -v gh >/dev/null || fail "GitHub CLI (gh) is required to verify release settings"
    origin_url="$(git remote get-url origin)" || fail "could not read the origin remote"
    case "$origin_url" in
        https://github.com/*)
            repository="${origin_url#https://github.com/}"
            ;;
        git@github.com:*)
            repository="${origin_url#git@github.com:}"
            ;;
        ssh://git@github.com/*)
            repository="${origin_url#ssh://git@github.com/}"
            ;;
        *)
            fail "origin must be a GitHub repository, got $origin_url"
            ;;
    esac
    repository="${repository%.git}"

    reviewers="$(gh api "repos/$repository/environments/underware-release" \
        --jq '[.protection_rules[]? | select(.type == "required_reviewers") | .reviewers[]?] | length')" || \
        fail "could not read the underware-release environment"
    [[ "$reviewers" =~ ^[1-9][0-9]*$ ]] || \
        fail "underware-release must require at least one reviewer"

    self_review="$(gh api "repos/$repository/environments/underware-release" \
        --jq '[.protection_rules[]? | select(.type == "required_reviewers") | .prevent_self_review] | index(false) | not')" || \
        fail "could not read underware-release self-review protection"
    [[ "$self_review" == "true" ]] || \
        fail "underware-release must prevent self-review"

    admin_bypass="$(gh api "repos/$repository/environments/underware-release" --jq '.can_admins_bypass')" || \
        fail "could not read underware-release administrator bypass setting"
    [[ "$admin_bypass" == "false" ]] || \
        fail "underware-release must not allow administrator bypass"

    main_reviews="$(gh api "repos/$repository/branches/main/protection" \
        --jq '.required_pull_request_reviews.required_approving_review_count')" || \
        fail "could not read main branch protection"
    [[ "$main_reviews" =~ ^[1-9][0-9]*$ ]] || \
        fail "main must require at least one approving review"

    main_checks="$(gh api "repos/$repository/branches/main/protection" \
        --jq '[.required_status_checks.contexts[]?] | index("release-policy") | not | not')" || \
        fail "could not read main required status checks"
    [[ "$main_checks" == "true" ]] || \
        fail "main must require the release-policy status check"

    main_admins="$(gh api "repos/$repository/branches/main/protection" --jq '.enforce_admins.enabled')" || \
        fail "could not read main administrator enforcement"
    force_pushes="$(gh api "repos/$repository/branches/main/protection" --jq '.allow_force_pushes.enabled')" || \
        fail "could not read main force-push protection"
    deletions="$(gh api "repos/$repository/branches/main/protection" --jq '.allow_deletions.enabled')" || \
        fail "could not read main deletion protection"
    [[ "$main_admins" == "true" && "$force_pushes" == "false" && "$deletions" == "false" ]] || \
        fail "main protection must apply to administrators and block force-pushes and deletion"

    tag_ruleset="$(gh api "repos/$repository/rulesets" \
        --jq '.[] | select(.name == "Underware release tags" and .target == "tag" and .enforcement == "active") | .id')" || \
        fail "could not read repository rulesets"
    [[ -n "$tag_ruleset" ]] || fail "active Underware release tag ruleset is missing"

    tag_rules="$(gh api "repos/$repository/rulesets/$tag_ruleset" --jq '[.rules[].type] | sort | join(",")')" || \
        fail "could not read the Underware release tag ruleset"
    [[ ",$tag_rules," == *,deletion,* && ",$tag_rules," == *,update,* ]] || \
        fail "Underware release tag ruleset must block updates and deletion"

    tag_bypasses="$(gh api "repos/$repository/rulesets/$tag_ruleset" --jq '.bypass_actors | length')" || \
        fail "could not read Underware release tag ruleset bypasses"
    [[ "$tag_bypasses" == "0" ]] || fail "Underware release tag ruleset must not allow bypasses"
}

check() {
    local version="$1"
    local tag="uw-v${version}"
    local base_version tag_state

    require_release_version "$version"
    require_clean_worktree
    require_main_tip
    require_release_settings
    require_remote_tag_absent "$tag"
    require_newer_than_published_release "$version"
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

    cargo build --release --bin torii
    verify_binary_version "$version" target/release/torii
}

verify_binary_version() {
    local version="$1" binary="$2"
    local base_version expected actual

    require_release_version "$version"
    [[ -x "$binary" ]] || fail "release binary is not executable: $binary"
    base_version="$(torii_base_version)"
    expected="torii ${version}-uw (base torii v${base_version}, $(git rev-parse --short HEAD))"
    actual="$("$binary" --version)"

    [[ "$actual" == "$expected" ]] || fail "unexpected binary version: $actual"
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
    require_newer_than_published_release "$version"
    git push origin "refs/tags/$tag"
    echo "pushed release candidate tag $tag"
}

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || fail "not inside a Git repository"
cd "$repo_root"

[[ $# -gt 0 ]] || {
    usage
    exit 2
}

case "$1" in
    check)
        [[ $# -eq 2 ]] || { usage; exit 2; }
        check "$2"
        ;;
    candidate)
        [[ $# -eq 2 ]] || { usage; exit 2; }
        candidate "$2"
        ;;
    validate-version)
        [[ $# -eq 2 ]] || { usage; exit 2; }
        require_release_version "$2"
        ;;
    validate-version-order)
        [[ $# -eq 2 ]] || { usage; exit 2; }
        require_release_version "$2"
        require_newer_than_published_release "$2"
        ;;
    verify-binary)
        [[ $# -eq 3 ]] || { usage; exit 2; }
        verify_binary_version "$2" "$3"
        ;;
    verify-settings)
        [[ $# -eq 1 ]] || { usage; exit 2; }
        require_release_settings
        echo "release settings verified"
        ;;
    *)
        usage
        exit 2
        ;;
esac
