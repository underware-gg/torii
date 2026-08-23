# Branching

## Remotes

Remotes are per-clone, so set them up as you prefer. The convention this project assumes:

| Name | Repository | Role |
|---|---|---|
| `origin` | `underware-gg/torii` | this fork; where our work is published |
| `upstream` | `dojoengine/torii` | canonical upstream; source of truth for releases |

This follows the usual fork convention: **`origin` is ours and `upstream` is canonical.** Local
`main` tracks `origin/main`; never rely on a bare `git push` until `git remote -v` confirms this
mapping.

## Branch model

- **`dev/<name>`** — working branches. Work lands here as one linear history on top of an upstream
  release. There is no per-change branch fan-out.
- **`main`** — the publishing point. Work releases into `main` when it is done.

Because `main` carries our commits, it diverges from upstream. Sync at an upstream release boundary
through a dedicated `sync/torii-<version>` branch cut from `main`: merge `upstream/main` into that
branch, resolve and test the result, then merge it to `main` through the normal reviewed pull
request. `main` is protected, never force-pushed, and keeps both the upstream and Underware
history needed to understand a release.

This is development synchronization only. Once a commit is on `main`, an Underware release builds
that local source and its `uw-v*` tag; it does not fetch or inspect `upstream`.

## Upstream contributions

Branches for upstream pull requests are cut deliberately, one at a time, when something is actually
being upstreamed. They are not maintained in parallel with our work — an earlier attempt to do that
produced history that had to be rewritten.
