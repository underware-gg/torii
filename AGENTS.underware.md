# Underware fork — agent entry point

Fork-specific orientation for [`underware-gg/torii`](https://github.com/underware-gg/torii),
Underware's fork of [`dojoengine/torii`](https://github.com/dojoengine/torii). Read alongside
[`AGENTS.md`](./AGENTS.md), which covers the upstream project itself.

This file routes; it does not duplicate. Policy detail lives in the contributor layer.

## What this fork is for

To **carry torii forward and fix issues in it**. Changes should be generic — things any torii
operator would want — and are offered upstream where practical.

Application-specific behaviour does not belong on the mainline branches. If that need arises, it
gets its own branch and a deliberate decision first.

The fork's immediate reason to exist is an indexer correctness fix; the reasoning is recorded in
[DD-0001](./docs/architecture/decisions/0001-indexer-task-dependency-and-rollback-cache-recovery.md).

## Before you change anything

| Read | For |
|---|---|
| [Branching](./docs/contributor/branching.md) | remote conventions — **`origin` is ours and `upstream` is canonical** — and the `dev/*` → `main` model |
| [Versioning](./docs/contributor/versioning.md) | the `uw-v` scheme, and why not to touch the `Cargo.toml` version |
| [Commit conventions](./docs/contributor/commit-conventions.md) | message format, history discipline, what is never committed |
| [Testing](./docs/contributor/testing.md) | **the suite needs fixtures extracted first**, or most of `torii-indexer` fails in a way that looks like a code fault |

## Documentation map

[`docs/README.md`](./docs/README.md) is the router for all four layers, and carries the canonical
statement of documentation-standard conformance and its one declared deviation.

## Recording decisions

Architectural decisions are decision records under
[`docs/architecture/decisions/`](./docs/architecture/decisions/README.md), with an immutable body
and supersede-rather-than-edit discipline. Read that router's conventions before adding one.
