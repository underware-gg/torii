# Underware fork policy

Fork-specific instructions for [`underware-gg/torii`](https://github.com/underware-gg/torii),
Underware's fork of [`dojoengine/torii`](https://github.com/dojoengine/torii). Read alongside
[`AGENTS.md`](./AGENTS.md), which covers the upstream project itself.

## Purpose and scope

This fork exists to **carry torii forward and fix issues in it**. Changes should be generic —
things any torii operator would want — and are offered upstream where practical.

Application-specific behaviour does not belong on the mainline branches. If that need arises, it
gets its own branch and a deliberate decision first.

The immediate reason the fork exists: two latent bugs combine on cold replay so that an
`ALTER TABLE` is silently skipped after a model upgrade, leaving torii's schema metadata claiming
a column that the SQLite table lacks, and breaking world indexing. The fix is offered upstream as
[`dojoengine/torii#428`](https://github.com/dojoengine/torii/pull/428).

## Remotes

Per-clone, so set these up as you prefer. The convention this project assumes:

| Name | Repository | Role |
|---|---|---|
| `origin` | `dojoengine/torii` | upstream; source of truth for releases |
| `underware` | `underware-gg/torii` | this fork; where our work is published |

Note the inversion from the usual convention: `origin` is upstream, not ours. Check
`git remote -v` before assuming.

## Branches

- **`dev/<name>`** — working branches. Our work lands here as one linear history on top of an
  upstream release. There is no per-change branch fan-out.
- **`main`** — the publishing point. Work releases into `main` when done.

Because `main` carries our commits it diverges from upstream, so syncing upstream is a **merge,
not a fast-forward**.

Upstream PR branches are cut deliberately, one at a time, when something is actually being
upstreamed — not maintained in parallel.

## Versioning

Two parallel version lines. Our version is ordinary semver that **never resets**; the upstream
base is **computed, not declared**.

- **Git tag:** `uw-v0.4.0`. The `uw-` prefix is required — the fork inherited every upstream
  `v1.8.x` tag, so ours must be distinguishable.
- **Version output:** `0.4.0-uw (base torii v1.8.16, <sha>)`, stamped at build time.
- **Upstream base:** `git merge-base HEAD origin/main`, described against upstream's tags.
- **Do not edit the workspace `version` in `Cargo.toml`.** Upstream bumps it in every
  `release(prepare)` commit; owning that line buys a merge conflict on every sync for no gain,
  since this fork publishes no crates.
- `0.x` while we track upstream. `1.0.0` is reserved for the point we stop tracking it.

## Commit conventions

- Conventional commits, matching upstream style: `fix(indexer):`, `refactor(processors):`,
  `test(storage):`, `docs:`.
- **No notes-only commits.** A previous workflow recorded promotion-target SHAs in separate notes
  commits; that produced history which had to be stripped out again. Working notes stay untracked.
- Keep each logical change as one commit. Prefer a readable linear history over preserving every
  intermediate step.
- Before pushing, confirm `git status --porcelain` is clean and
  `git ls-files --others --exclude-standard` is empty.

## Never commit

Enforced by `.gitignore` — do not weaken it:

- `.dev.local/` — working notes
- `/tmp/` — build and test scratch
- `*.local.md` — machine-specific agent overrides

## Testing

**The test suite needs fixtures extracted first**, or most of `torii-indexer` fails in a way that
looks like a code fault:

```sh
bash scripts/extract_test_db.sh
```

See [`docs/contributor/testing.md`](./docs/contributor/testing.md) for the full setup and what
each suite covers.

## Documentation

This project conforms partially to the Agent-Ready Documentation Standard v1.0 at **Level 1
(Bootstrapped)**. Start at [`docs/README.md`](./docs/README.md).

Conformance is scoped to the contributor layer, which carries real content. The user, functional
and architecture layers are stubs: they describe torii itself, which is largely upstream's
software and upstream's to document. They grow as our divergence grows.
