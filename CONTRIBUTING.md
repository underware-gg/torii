# Contributing

This is **Underware's fork of [torii](https://github.com/dojoengine/torii)**, the Dojo indexer.
Its purpose is to carry torii forward and fix issues in it.

Changes here should be **generic** — things any torii operator would want. Application-specific
behaviour does not belong on the mainline branches; if that need arises it gets its own branch and
a deliberate decision first. Where practical, fixes are also offered upstream.

## Start here

- [`docs/contributor/README.md`](./docs/contributor/README.md) — the contributor layer index
- [`docs/contributor/testing.md`](./docs/contributor/testing.md) — **read before running tests**; the suite needs fixtures extracted first
- [`docs/README.md`](./docs/README.md) — all documentation, and our documentation-standard conformance

## The essentials

- Branch from `main` onto a `dev/<name>` branch. **`origin` is this Underware fork and `upstream`
  is canonical Torii** — see [Branching](./docs/contributor/branching.md).
- Conventional commits, matching upstream style. No notes-only commits — see
  [Commit conventions](./docs/contributor/commit-conventions.md).
- Do not edit the workspace `version` in `Cargo.toml` — see
  [Versioning](./docs/contributor/versioning.md).
- Architectural decisions get a decision record — see
  [`docs/architecture/decisions/`](./docs/architecture/decisions/README.md).

Agents: start at [`AGENTS.underware.md`](./AGENTS.underware.md).
