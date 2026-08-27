# Testing

## CI policy checks

The repository keeps semantic review with the developer responsible for merging. CI does not use
agent API keys, OAuth tokens, or an automated reviewer. The required `release-policy` job instead
runs deterministic checks for the first-party shell and GitHub Actions files that control CI and
releases.

Install the local prerequisites once on macOS:

```sh
brew install actionlint shellcheck
bin/setup-githooks
```

CI pins actionlint 1.7.12 and ShellCheck 0.11.0. Compatible local versions can run the same check:

```sh
./scripts/check_ci_policy.sh
```

Shell checks cover `.githooks/`, `bin/setup-githooks`, and `scripts/*.sh` at warning-or-higher
severity. actionlint checks every active workflow. Its configuration contains one narrow temporary
exception for `release.yml`: GitHub supports `concurrency.queue: max`, but actionlint 1.7.12 does
not parse the key yet. Remove the exception when the pinned stable actionlint release supports it.

The pre-commit hook runs this policy check when a relevant staged path changes. It validates the
working-tree files, so restage any file the check causes you to fix. A reviewer must not approve,
and a maintainer must not bypass the review requirement, while any triggered check is failing;
`release-policy` remains the sole mechanically required status check.

`main` has two protection layers. The base branch protection requires a pull request, resolved
review conversations, and a passing `release-policy` check for everyone, including administrators.
An additional review-only ruleset requires one approval but gives the `underware-gg/admin` team a
**For pull requests only** bypass. Admins may use that recorded bypass for their own maintainer PRs
or exceptional cases; it never permits a direct push or skips the base protections. Other
contributors require one approving review from a collaborator with write access.

## The fixture requirement

Most of the `torii-indexer` suite needs test databases extracted to `/tmp` first. Without them
tests fail at `dojo_test_utils::migration` with `Failed to copy directory: No such file or
directory`, which looks like a code fault and is not:

```sh
bash scripts/extract_test_db.sh
```

This untars `spawn-and-move-db.tar.gz` and `types-test-db.tar.gz` into `/tmp/`. It touches nothing
in the working tree. Upstream's CI runs it before every test job; it is easy to miss locally
because nothing else prompts you to.

Measured on `dev/rob` (2026-08-23): without the fixture, `cargo test -p torii-indexer` gave
12 failed / 1 passed. With it, **13 passed / 0 failed**.

## Full local setup

Mirroring upstream's CI test job:

```sh
# Cairo toolchain and a Katana binary on PATH are required for the integration tests.
# See .github/workflows/test.yml for the versions CI pins.
sozo build --manifest-path examples/spawn-and-move/Scarb.toml
sozo build --manifest-path crates/types-test/Scarb.toml
bash scripts/extract_test_db.sh
```

The `sozo build` steps are only needed if `examples/spawn-and-move/target` or
`crates/types-test/target` are absent.

## Running

```sh
cargo check --workspace --all-targets
cargo test -p torii-task-network   # task dependency preservation
cargo test -p torii-indexer        # rollback cache recovery; needs the fixture above
cargo nextest run --all-features --workspace   # what CI runs
```
