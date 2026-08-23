# Architecture

How the system is structured, and why.

torii's overall architecture is upstream's and documented there. This layer covers the parts we
change and the reasoning behind our changes.

| Artefact | Summary |
|---|---|
| [Decision records](./decisions/README.md) | Router for all architectural decision records |

## Subsystems we have modified

- **Indexer task scheduling** — `crates/processors/src/task_manager.rs`, `crates/task-network/`
- **Indexer rollback and cache recovery** — `crates/indexer/engine/`, `crates/cache/`,
  `crates/storage/`, `crates/sqlite/sqlite/`

Both are covered by
[DD-0001](./decisions/0001-indexer-task-dependency-and-rollback-cache-recovery.md).

Architectural overviews and cross-cutting patterns belong in this subtree as they are written.
