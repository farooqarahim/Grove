---
id: plan
name: Plan Mode
description: Requirements + design only. No code changes. Use when exploring what to build.
default: false
agents:
  - build_prd
  - plan_system_design
gates:
  - build_prd
aliases:
  - plan-only
  - plan_only
  - docs
  - investigate
  - review-only
  - review_only
  - security-audit
  - security_audit
---

# Plan Mode Pipeline

Runs two agents sequentially to produce planning artifacts without writing any code.

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
       ▼
  [Complete — no code changes]
```

## When to Use

- Exploring a new feature before committing to implementation
- Getting architecture feedback before writing code
- Creating documentation for a project or feature
- Planning a migration or refactor strategy

## Output

Two markdown documents in `.grove/artifacts/{conversation_id}/{run_id}/`:
1. `GROVE_PRD_{run_id}.md` — Product Requirements Document
2. `GROVE_DESIGN_{run_id}.md` — Technical System Design

These can be fed into **Build Mode** later for implementation.
