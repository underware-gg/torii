# DD-0001 — Indexer task dependency and rollback cache recovery

- **Status:** accepted
- **Date:** 2026-05-01
- **Supersedes:** —
- **Superseded by:** —
- **Upstream:** offered as [`dojoengine/torii#428`](https://github.com/dojoengine/torii/pull/428)

## Context

Two latent bugs in torii's indexer combine on cold replay. Both have been present in every release
since `v1.5.0` (April 2025), tracing to changes in tree by November 2024, and neither is visible
under continuous indexing.

**Task/dependency loss.** In `TaskManager` and `TaskNetwork`, when earlier same-player events have
already created a historical replay task before an `EventUpgraded` lands, later post-upgrade events
are appended to the existing task without acquiring the selector-upgrade dependency. The task then
deserialises later events against the old cached schema, producing
`PrimitiveError(InvalidEnumSelector)` whenever a payload uses a discriminant that only exists after
the upgrade.

**Rollback/cache divergence.** When a chunk fails after a `ModelUpgraded` has been processed, the
queued SQL — including `ALTER TABLE` — is rolled back, but the in-memory model cache is not. On
retry, the upgrade processor reads the post-upgrade schema from the poisoned cache, sees no
difference against the chain schema, and silently skips the `ALTER TABLE`. The result is schema
metadata that reports a field the SQLite table does not have.

Together they poison a database irrecoverably during cold replay: the deserialize failure supplies
the error, and the cache divergence ensures the retry never re-issues the missing migration.

## Decision

Fix both, in four parts:

1. `TaskManager::add_parallelized_event_with_dependencies` merges newly discovered dependencies
   into an existing task instead of only appending the event.
2. `TaskNetwork` retains unresolved dependencies and activates them once the prerequisite task is
   inserted, instead of dropping them as non-existent.
3. Engine rollback handling resets commit-sensitive cache state to what is committed in storage.
4. Processors resolve model definitions through storage rather than the cache, so a rollback-time
   cache reset repopulates from committed SQLite instead of failing to find the model.

Parts 1–2 close the trigger. Parts 3–4 close the divergence — clearing the cache alone is
insufficient, because processors then fail to find model definitions in an empty cache.

## Alternatives considered

**Wait for upstream.** Rejected. The failure had been live for months, and upstream torii v1 is in
maintenance with the successor effort in a separate repository. Waiting is what produced the
outage.

**Reduce `blocks_chunk_size` as a permanent workaround.** Separating the offending event from the
model upgrade into different chunks avoids the trigger. Rejected as a fix: it does not address the
hazard, which remains latent for any future error inside an upgrade chunk, and it multiplies commit
cycles during precisely the cold replay it is meant to protect. Retained only as an operational
mitigation for unpatched deployments.

**Ignore unknown enum selectors during deserialisation.** Rejected: it masks genuine schema
mismatch and would let a poisoned database continue silently.

**Bypass the cache when reading a previous schema.** Rejected as too narrow — it addresses the read
path without fixing the divergence that put the cache in a wrong state.

## Consequences

- Cold replay across a model upgrade completes rather than poisoning the database.
- Processors gain a dependency on storage for model resolution; storage is cache-first, so the
  steady-state read path is unchanged.
- We carry a fork until the change lands upstream. See
  [Versioning](../../contributor/versioning.md) for how our releases are numbered against an
  upstream base.
- Existing databases already poisoned by this path are not repaired by the fix; they require a
  fresh index.
