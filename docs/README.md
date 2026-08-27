# Documentation

Documentation router for Underware's fork of [torii](https://github.com/dojoengine/torii).

## Layers

- [User](./user/README.md) — installing, configuring and running this build
- [Functional](./functional/README.md) — what torii does, and where our behaviour differs
- [Architecture](./architecture/README.md) — structure and the decisions behind our changes
- [Contributor](./contributor/README.md) — how to work in this fork

## Convention-based exceptions

- [`CONTRIBUTING.md`](../CONTRIBUTING.md) — the contributor layer's public-facing landing page
- [`AGENTS.underware.md`](../AGENTS.underware.md) — agent-facing entry point for this fork, reached from `AGENTS.md`

## Conformance

Canonical statement; cited from elsewhere rather than repeated.

This project conforms to the **Agent-Ready Documentation Standard v1.0** at **Level 1
(Bootstrapped)**, partially, with one declared deviation.

**Scope.** The contributor layer carries substantive content, because it describes how we work in
this fork. The user, functional and architecture layers are deliberately thin: they describe torii
itself, which is largely upstream's software and upstream's to document. They grow as our
divergence grows. We do not restate upstream's documentation here.

**Declared deviation — the bootstrap.** The standard's bootstrap contract asks the entry point to
name the project, link each layer, and name convention-based exceptions. Our discovery entry point
is `AGENTS.md`, which is **upstream's file**. We deliberately limit our footprint there to a single
pointer line, so the bootstrap contract is satisfied by delegation:
`AGENTS.md` → [`AGENTS.underware.md`](../AGENTS.underware.md) → this router. Layer links are
therefore two hops from discovery rather than one. This is a considered trade of strict conformance
for minimal merge surface against upstream, not an oversight.

## External context

The standard itself is maintained outside this repository. Per its own guidance on external
context, this documentation does not depend on it for completeness — an agent can work here
without reading it.
