# Commit conventions

## Messages

Conventional commits, matching upstream's style: `fix(indexer):`, `refactor(processors):`,
`test(storage):`, `docs:`, `chore:`.

## History discipline

- Keep each logical change as one commit. Prefer a readable linear history over preserving every
  intermediate step.
- **No notes-only commits.** An earlier workflow recorded promotion-target SHAs in separate
  notes commits; the result was history that had to be stripped out again. Working notes stay
  untracked.
- Before pushing, confirm `git status --porcelain` is clean. It reports tracked, untracked, and
  staged changes (except intentionally ignored files).

## Never committed

Enforced by `.gitignore` — do not weaken it:

| Pattern | What it holds |
|---|---|
| `.dev.local/` | working notes, task queues, draft PR wording |
| `/tmp/` | build and test scratch, including extracted test fixtures |
| `*.local.md` | machine-specific agent overrides |
