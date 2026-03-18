---
id: build
name: Build Mode
description: Implementation + quality gates. Use when you already have a plan or know what to build.
default: false
agents:
  - builder
  - reviewer
  - judge
gates: []
aliases:
  - build-mode
  - instant
  - quick
  - prototype
  - bugfix
  - ci-fix
  - ci_fix
  - refactor
  - test-coverage
  - test_coverage
---

# Build Mode Pipeline

Runs three agents sequentially: implement, review, and judge. No planning phase — assumes you know what to build.

## Flow

```
[User Objective]
       │
       ▼
  ┌─────────┐
  │ Builder  │  → writes code, runs tests
  └─────────┘
       │
       ▼
  ┌──────────┐
  │ Reviewer  │  → audits code, fixes critical issues, writes GROVE_REVIEW_{run_id}.md
  └──────────┘
       │
       ▼
  ┌─────────┐
  │  Judge   │  → final quality check, writes GROVE_VERDICT_{run_id}.md
  └─────────┘
       │
       ▼
  [Complete]
```

## When to Use

- Bug fixes where the problem is well-understood
- Small features that don't need a design phase
- Refactoring tasks with clear scope
- When you've already run Plan Mode and have the design docs in the worktree

## Output

- Code changes in the worktree
- `GROVE_REVIEW_{run_id}.md` — Code review with PASS/FAIL verdict (in `.grove/artifacts/`)
- `GROVE_VERDICT_{run_id}.md` — Final quality verdict (APPROVED/NEEDS_WORK/REJECTED) (in `.grove/artifacts/`)
