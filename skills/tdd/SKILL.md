---
name: tdd
description: Test-Driven Development workflow for the Builder agent. Ensures code is written test-first with proper red-green-refactor cycles.
applies_to:
  - builder
---

# Test-Driven Development

Follow this cycle for every piece of new functionality:

## The Red-Green-Refactor Cycle

### 1. Red — Write a Failing Test First

Before writing any implementation:
- Write a test that describes the expected behavior
- Run it to confirm it fails (for the right reason)
- The failure message should clearly state what's missing

### 2. Green — Write Minimal Code to Pass

- Write the simplest implementation that makes the test pass
- Do not add features the test doesn't require
- Do not optimize prematurely
- Run the test to confirm it passes

### 3. Refactor — Clean Up

- Remove duplication
- Improve naming
- Extract functions if needed
- Run tests again to confirm nothing broke

## Rules

1. **Never write implementation before the test** — the test defines the contract
2. **One behavior per test** — test names should read like specifications
3. **Test error paths too** — invalid input, missing data, network failures
4. **Tests must be independent** — no shared mutable state between tests
5. **Tests must be fast** — mock external services, use in-memory databases

## Test Structure

```
// Arrange — set up preconditions
// Act — call the function under test
// Assert — verify the result
```

## When to Skip TDD

- Exploratory/prototype code (but add tests before merging)
- Trivial getters/setters
- UI layout code (use visual testing instead)
