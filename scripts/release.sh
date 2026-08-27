#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  ./scripts/release.sh check <version>
  ./scripts/release.sh candidate <version>
  ./scripts/release.sh validate-version <version>
  ./scripts/release.sh validate-candidate <version>
  ./scripts/release.sh validate-queue [<run-id> <run-number>]
  ./scripts/release.sh is-highest-published-version <version>
  ./scripts/release.sh validate-version-order <version>
  ./scripts/release.sh verify-binary <version> <binary>
  ./scripts/release.sh verify-settings

Commands:
  check      Verify that the checked-out main commit is ready for a release candidate.
  candidate  Run check and dispatch the protected build-before-tag release workflow.
  validate-version
             Verify that a version is canonical Underware release semver.
  validate-candidate
             Verify that a version is canonical, newer, and unused on origin.
  validate-queue
             Verify that no earlier release candidate is still active.
  is-highest-published-version
             Print whether a version is at least as high as every published release.
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

require_remote_tag_absent() {
    local tag="$1" remote_tag

    remote_tag="$(git ls-remote --tags origin "refs/tags/$tag")" || \
        fail "could not query origin for $tag"
    [[ -z "$remote_tag" ]] || fail "$tag already exists on origin"
}

require_release_candidate() {
    local version="$1"
    local tag="uw-v${version}"

    require_release_version "$version"
    require_remote_tag_absent "$tag"
    require_newer_than_published_release "$version"
}

github_repository() {
    local origin_url repository

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
    printf '%s\n' "${repository%.git}"
}

require_newer_than_published_release() {
    local version="$1" is_highest

    is_highest="$(release_tag_version_is_highest "$version")"
    [[ "$is_highest" == "true" ]] || \
        fail "version $version must not be older than an existing Underware release"
}

version_is_highest_among() {
    local version="$1" versions="$2" other_version
    local candidate_major candidate_minor candidate_patch published_major published_minor published_patch
    local is_highest="true"

    IFS=. read -r candidate_major candidate_minor candidate_patch <<<"$version"
    while IFS= read -r other_version; do
        [[ -n "$other_version" ]] || continue
        require_release_version "$other_version"

        IFS=. read -r published_major published_minor published_patch <<<"$other_version"
        if ((10#$candidate_major < 10#$published_major ||
            (10#$candidate_major == 10#$published_major && 10#$candidate_minor < 10#$published_minor) ||
            (10#$candidate_major == 10#$published_major && 10#$candidate_minor == 10#$published_minor && 10#$candidate_patch < 10#$published_patch))); then
            is_highest="false"
        fi
    done <<<"$versions"

    printf '%s\n' "$is_highest"
}

release_tag_version_is_highest() {
    local version="$1" remote_tags tag versions=""

    remote_tags="$(git ls-remote --refs --tags origin 'refs/tags/uw-v*')" || \
        fail "could not query origin release tags"
    while IFS=$'\t' read -r _ tag; do
        [[ -n "$tag" ]] || continue
        versions+="${tag#refs/tags/uw-v}"$'\n'
    done <<<"$remote_tags"

    version_is_highest_among "$version" "$versions"
}

published_release_version_is_highest() {
    local version="$1" repository published_tags tag versions=""

    command -v gh >/dev/null || fail "GitHub CLI (gh) is required to read published releases"
    repository="$(github_repository)"
    published_tags="$(gh api --paginate "repos/$repository/releases?per_page=100" \
        --jq '.[] | select(.draft == false and .prerelease == false and (.tag_name | startswith("uw-v"))) | .tag_name')" || \
        fail "could not read published Underware releases"
    while IFS= read -r tag; do
        [[ -n "$tag" ]] || continue
        versions+="${tag#uw-v}"$'\n'
    done <<<"$published_tags"

    version_is_highest_among "$version" "$versions"
}

require_release_queue_head() {
    local current_run_id="${1:-}" current_run_number="${2:-}"
    local repository active_runs filter

    command -v gh >/dev/null || fail "GitHub CLI (gh) is required to verify the release queue"
    repository="$(github_repository)"

    if [[ -z "$current_run_id" && -z "$current_run_number" ]]; then
        filter='[.workflow_runs[] | select(.status != "completed")] | length'
    else
        [[ "$current_run_id" =~ ^[0-9]+$ && "$current_run_number" =~ ^[0-9]+$ ]] || \
            fail "release run id and number must be numeric"
        filter="[.workflow_runs[] | select(.status != \"completed\" and .id != $current_run_id and .run_number < $current_run_number)] | length"
    fi

    active_runs="$(gh api "repos/$repository/actions/workflows/release.yml/runs?per_page=100" \
        --jq "$filter")" || fail "could not read the Underware release queue"
    [[ "$active_runs" == "0" ]] || \
        fail "an earlier Underware release candidate is still active; wait for it to complete"
}

require_release_settings() {
    local repository reviewers self_review admin_bypass
    local main_reviews main_checks main_admins force_pushes deletions conversations
    local review_ruleset review_scope review_rule admin_team_id review_bypasses
    local tag_ruleset tag_scope tag_rules tag_bypasses

    command -v gh >/dev/null || fail "GitHub CLI (gh) is required to verify release settings"
    repository="$(github_repository)"

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
    [[ "$main_reviews" == "0" ]] || \
        fail "main branch protection must require pull requests without a universal approval gate"

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
    conversations="$(gh api "repos/$repository/branches/main/protection" \
        --jq '.required_conversation_resolution.enabled')" || \
        fail "could not read main conversation-resolution protection"
    [[ "$main_admins" == "true" && "$force_pushes" == "false" && "$deletions" == "false" && "$conversations" == "true" ]] || \
        fail "main protection must apply to administrators, resolve conversations, and block force-pushes and deletion"

    review_ruleset="$(gh api "repos/$repository/rulesets" \
        --jq '[.[] | select(.name == "Underware main reviews" and .target == "branch" and .enforcement == "active")] | if length == 1 then .[0].id else empty end')" || \
        fail "could not read repository rulesets"
    [[ -n "$review_ruleset" ]] || fail "active Underware main reviews ruleset is missing or duplicated"

    review_scope="$(gh api "repos/$repository/rulesets/$review_ruleset" \
        --jq '(.conditions.ref_name.include == ["refs/heads/main"]) and ((.conditions.ref_name.exclude // []) | length == 0)')" || \
        fail "could not read Underware main reviews ruleset conditions"
    [[ "$review_scope" == "true" ]] || \
        fail "Underware main reviews ruleset must apply only to refs/heads/main"

    review_rule="$(gh api "repos/$repository/rulesets/$review_ruleset" --jq '
        (.rules | length == 1) and
        (.rules[0].type == "pull_request") and
        (.rules[0].parameters.required_approving_review_count == 1) and
        (.rules[0].parameters.dismiss_stale_reviews_on_push == true) and
        (.rules[0].parameters.require_code_owner_review == false) and
        (.rules[0].parameters.require_last_push_approval == false) and
        (.rules[0].parameters.required_review_thread_resolution == false)')" || \
        fail "could not read Underware main review requirements"
    [[ "$review_rule" == "true" ]] || \
        fail "Underware main reviews ruleset must contain only the one-approval review rule"

    admin_team_id="$(gh api "repos/$repository/teams" \
        --jq '[.[] | select(.slug == "admin" and .permission == "admin")] | if length == 1 then .[0].id else empty end')" || \
        fail "could not read repository teams"
    [[ -n "$admin_team_id" ]] || fail "repository must have exactly one admin team with admin permission"
    review_bypasses="$(gh api "repos/$repository/rulesets/$review_ruleset" \
        --jq '[.bypass_actors[] | "\(.actor_type):\(.actor_id):\(.bypass_mode)"] | sort | join(",")')" || \
        fail "could not read Underware main review bypasses"
    [[ "$review_bypasses" == "Team:$admin_team_id:pull_request" ]] || \
        fail "Underware main reviews must allow only the admin team to bypass from a pull request"

    tag_ruleset="$(gh api "repos/$repository/rulesets" \
        --jq '[.[] | select(.name == "Underware release tags" and .target == "tag" and .enforcement == "active")] | if length == 1 then .[0].id else empty end')" || \
        fail "could not read repository rulesets"
    [[ -n "$tag_ruleset" ]] || fail "active Underware release tag ruleset is missing or duplicated"

    tag_scope="$(gh api "repos/$repository/rulesets/$tag_ruleset" \
        --jq '((.conditions.ref_name.include // []) | index("refs/tags/uw-v*") != null) and ((.conditions.ref_name.exclude // []) | length == 0)')" || \
        fail "could not read Underware release tag ruleset conditions"
    [[ "$tag_scope" == "true" ]] || \
        fail "Underware release tag ruleset must include refs/tags/uw-v* with no exclusions"

    tag_rules="$(gh api "repos/$repository/rulesets/$tag_ruleset" --jq '[.rules[].type] | sort | join(",")')" || \
        fail "could not read the Underware release tag ruleset"
    [[ ",$tag_rules," == *,deletion,* && ",$tag_rules," == *,update,* ]] || \
        fail "Underware release tag ruleset must block updates and deletion"

    tag_bypasses="$(gh api "repos/$repository/rulesets/$tag_ruleset" --jq '.bypass_actors | length')" || \
        fail "could not read Underware release tag ruleset bypasses"
    [[ "$tag_bypasses" == "0" ]] || fail "Underware release tag immutability must not allow bypasses"

}

check() {
    local version="$1"
    local tag="uw-v${version}"
    local base_version

    require_clean_worktree
    require_main_tip
    require_release_settings
    require_release_candidate "$version"
    base_version="$(torii_base_version)"
    [[ -n "$base_version" ]] || fail "could not read workspace Cargo version"

    echo "release check passed"
    echo "  commit: $(git rev-parse --short HEAD)"
    echo "  tag: $tag (absent on origin)"
    echo "  torii base: v$base_version"
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
    local repository

    check "$version"
    require_release_queue_head
    repository="$(github_repository)"
    gh workflow run release.yml --repo "$repository" --ref main \
        -f "version=$version" -f "commit=$(git rev-parse HEAD)"
    echo "dispatched release candidate $tag for $(git rev-parse --short HEAD)"
    echo "the immutable tag will be created only after builds pass and publication is approved"
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
    validate-candidate)
        [[ $# -eq 2 ]] || { usage; exit 2; }
        require_release_candidate "$2"
        ;;
    validate-queue)
        if [[ $# -eq 1 ]]; then
            require_release_queue_head
        elif [[ $# -eq 3 ]]; then
            require_release_queue_head "$2" "$3"
        else
            usage
            exit 2
        fi
        ;;
    is-highest-published-version)
        [[ $# -eq 2 ]] || { usage; exit 2; }
        require_release_version "$2"
        published_release_version_is_highest "$2"
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
