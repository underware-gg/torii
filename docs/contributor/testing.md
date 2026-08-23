# Testing

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
