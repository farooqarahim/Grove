---
id: autonomous
name: Autonomous Mode
description: Full end-to-end pipeline. PRD → Design → Build → Review → Judge.
default: true
agents:
  - build_prd
  - plan_system_design
  - builder
  - reviewer
  - judge
gates:
  - build_prd
  - plan_system_design
aliases:
  - auto
  - full
  - standard
  - secure
  - hardened
  - enterprise
  - fullstack
  - parallel-build
  - parallel_build
  - migration
  - cleanup
---

# Autonomous Mode Pipeline

Runs all five agents end-to-end with review gates after planning before building.

## Flow

```
[User Objective]
       │
       ▼
  ┌─────────┐
  │ BuildPrd │  → writes GROVE_PRD_{run_id}.md
  └─────────┘
       │
   [GATE: user review]
       │
       ▼
  ┌──────────────────┐
  │ PlanSystemDesign  │  → writes GROVE_DESIGN_{run_id}.md
  └──────────────────┘
       │
   [GATE: user review]
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

- New features that need full planning and implementation
- Complex tasks where you want every stage documented
- When you want human review checkpoints before code is written

## Gates

Two review gates pause execution for user approval:
1. **After BuildPrd** — review the PRD before designing
2. **After PlanSystemDesign** — review the design before building

## Output

All document artifacts are written to `.grove/artifacts/{conversation_id}/{run_id}/`:
- `GROVE_PRD_{run_id}.md` — Product Requirements Document
- `GROVE_DESIGN_{run_id}.md` — Technical System Design
- `GROVE_REVIEW_{run_id}.md` — Code review verdict
- `GROVE_VERDICT_{run_id}.md` — Final quality verdict
- Code changes are written to the worktree as usual
