---
uid: feat-016
status: open
priority: high
timeEstimate: 960
tags:
  - task
  - feature
  - schema
  - types
projects:
  - "[[sprint.md|Backlog]]"
contexts:
  - schema
  - typescript
  - contracts
---

# Contract drift detection between ORM models and TypeScript types

## Description

Detect mismatches between ORM-backed data models and TypeScript contracts that are supposed to represent the same shape across API and application boundaries.

## Motivation

A generic "type comparison" engine would be too broad and brittle. The valuable use case is contract-oriented drift detection: database model vs DTO, ORM model vs Zod schema, or backend entity vs frontend-facing TypeScript interface.

## Proposed Outcome

Start with targeted comparisons such as:

1. Prisma or Drizzle model to DTO or interface
2. ORM model to Zod schema
3. backend entity to frontend contract type

The feature should highlight:

1. missing fields
2. type mismatches
3. nullability mismatches
4. relations present in one side but absent in the other
5. likely stale contracts after model evolution

## Likely Scope

- explicit contract pairing configuration
- structural comparison model
- normalized representation for supported ORM and TS contract sources
- CLI or report surface for drift findings
- fixtures for real-world mismatch cases

## Subtasks

- [ ] Define the supported first-class sources and pairings
- [ ] Decide how contract mapping is configured explicitly
- [ ] Normalize field, nullability, and relation metadata for comparison
- [ ] Implement mismatch reporting with useful diagnostics
- [ ] Add fixtures for Prisma, Drizzle, and Zod-oriented cases if supported
- [ ] Document recommended usage and limitations

## Notes

This should be scoped as contract drift detection, not a universal semantic type engine. The first design decision is whether the primary target is backend-to-API drift or backend-to-frontend drift.
