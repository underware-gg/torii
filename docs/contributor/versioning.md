# Versioning

Two parallel version lines: **our version is ordinary semver that never resets**, and the
**Torii base is inherited from the source being released**.

## Scheme

- **Git tag:** `uw-v0.4.0`. The `uw-` prefix is required — this fork inherited every upstream
  `v1.8.x` tag, so ours must be distinguishable.
- **Version output:** `0.4.0-uw (base torii v1.8.16, <sha>)`, stamped at build time.
- **Torii base:** the workspace Cargo version in the exact Underware commit being built. It is
  inherited from the upstream source we integrated; the fork never maintains it separately.

The `torii` binary stamps this single value at build time. Ordinary builds take the newest reachable
`uw-v*` tag for our version; the protected release workflow supplies its validated candidate version
before the final tag exists. Both read the Torii base from the source's Cargo metadata and include
the build's HEAD SHA. The workflow creates the matching tag at that exact commit only after every
release build succeeds. The same value is used by `torii --version`, the service endpoint, the HTTP
User-Agent, and snapshot compatibility checks. Without a reachable fork tag or an explicit release
version, the version is `unreleased-uw`; missing SHA metadata is reported as `unknown` rather than
guessed.

## Rules

- **Do not edit the workspace `version` in `Cargo.toml`.** Upstream bumps it in every
  `release(prepare)` commit; owning that line buys a merge conflict on every sync for no gain,
  since this fork publishes no crates.
- **Our counter never resets or reuses a release identity.** Every distinct released commit gets a
  new version, including a sync to a newer upstream release. Once `uw-v0.4.0` exists, a
  rebuilt base must become (for example) `0.5.0`; a release must be greater than every existing
  Underware release tag. Tags are immutable evidence of the commit they released. A resetting or
  reused counter would misrepresent our own history.
- **`0.x` while we track upstream.** `1.0.0` is reserved for the point at which we stop tracking it
  — see below.

## Releasing

Promote the validated `dev/*` branch to `main` through the normal reviewed workflow. Release is a
separate action from that promotion: check out the published local `main`, then run:

```bash
./scripts/release.sh verify-settings
./scripts/release.sh check 0.4.0
./scripts/release.sh candidate 0.4.0
```

`verify-settings` confirms that GitHub has a peer-approved release environment with no bypass, a
non-force-pushable `main` with mandatory pull requests and the required `release-policy` check, a
one-approval review ruleset with a pull-request-only bypass for the `underware-gg/admin` team, and
immutable release tags. `check` performs the same safeguard check, and is otherwise read-only apart
from refreshing the local `origin/main` reference. It requires a clean working tree, local `main` at
exactly `origin/main`, and no remote `uw-v<version>` tag. `candidate` then dispatches the protected
release workflow for that version and exact commit. It does not create or push a tag.

Release workflows use GitHub Actions' multi-run FIFO queue and run one at a time. Pending candidates
are retained rather than replaced. Each candidate runs the full test suite, builds and verifies all
release platforms, stores the resulting artifacts, and verifies the multi-platform container build
before publication can be approved.

Publication requires approval through the protected `underware-release` GitHub Environment. Only
after the builds pass and approval is granted does the workflow create the canonical annotated tag
at the tested commit, create or update the draft GitHub release from those exact artifacts, publish
the Docker image, and publish the release. The publication steps are idempotent: if an external
service fails after tag creation, rerun the same job with the same version rather than changing the
immutable tag. A failure before publication approval creates no tag and consumes no version.

The final tag records the Underware version; never hand-edit it into `Cargo.toml` or source code. A
release build does not fetch or inspect official Torii. Publication order ensures the `latest`
container tag always represents the last approved Underware release.

Before the first release, configure the base `main` protection and its separate review-only ruleset,
protect `uw-v*` tags against update and deletion, and configure `underware-release` with required
reviewers. `verify-settings` makes these external requirements observable; without the environment
protection, GitHub does not pause the publish job.

## Why not encode both in one version string

A composite such as `1.8.16-uw.0.1.0` forces our counter to reset on every rebase, and sorts
*below* plain `1.8.16` because anything after a hyphen is a semver pre-release. Four-component
(`1.8.17.1`) and trailing-letter (`1.8.17a`) forms are not valid semver at all. Keeping the two
lines separate avoids all of it, and degrades gracefully if we stop tracking upstream.

## Stopping tracking upstream

`1.0.0` marks the point where this is no longer a downstream fork. The trigger is a decision, not
an event: we have crossed it when we take a change we would never upstream. Cut it at an upstream
sync boundary so history has a clean seam. At that point we also take ownership of the `Cargo.toml`
version, since there is nothing left to merge.
