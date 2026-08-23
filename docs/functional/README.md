# Functional

Reference-oriented: what the system does.

For a fork, most of this layer is upstream's. The functional surface is documented by torii's own
crate-level documentation under [`crates/`](../../crates) and, per the standard's treatment of
tests as documentation, by the test suites that assert behaviour — see
[Testing](../contributor/testing.md) for what each suite covers.

## Our behavioural delta

| Artefact | Summary |
|---|---|
| _none_ | Our changes to date are indexer **correctness** fixes: they add no feature and change no intended behaviour, but make cold replay behave as already specified. The reasoning is recorded as [DD-0001](../architecture/decisions/0001-indexer-task-dependency-and-rollback-cache-recovery.md). |

Document a feature here when we add or change one, rather than restating upstream.
