# Decision records

Router for architectural decision records (DDs) in this fork.

A decision record preserves the reasoning behind a non-obvious choice, so later contributors
recover context without reconstructing it from diffs.

## Conventions

- **Naming:** `NNNN-kebab-case-title.md`, numbered sequentially from `0001`. Numbers are never
  reused or renumbered.
- **Scope:** cross-cutting decisions live here at the layer root. A decision governing one
  subsystem may instead sit beside it; this router indexes it either way.
- **Immutable body.** Context, Decision, Alternatives and Consequences are frozen as written. They
  are not rewritten to match current reality — current behaviour belongs in living docs, the
  historical *why* belongs here.
- **Supersede and extend.** A later decision never edits an earlier record. It supersedes
  (replaces the decision) or extends (builds on it). Only the navigational header — status,
  supersedes, superseded-by, extended-by — is updated.
- **Status:** `accepted` · `superseded` · `extended`.

## Index

| ID | Title | Status | Summary |
|---|---|---|---|
| [DD-0001](./0001-indexer-task-dependency-and-rollback-cache-recovery.md) | Indexer task dependency and rollback cache recovery | accepted | Why we carry a four-part indexer fix rather than waiting for upstream |

## Related decisions held outside this repository

Fork policy decisions — the versioning scheme and the condition under which we stop tracking
upstream — are recorded in Underware's internal knowledge base and summarised in
[Versioning](../../contributor/versioning.md). They are policy rather than architecture, so they
are not DDs.
