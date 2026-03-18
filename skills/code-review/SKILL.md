---
name: code-review
description: Structured code review checklist for the Reviewer agent. Provides a systematic framework for evaluating code changes across correctness, security, performance, and maintainability.
applies_to:
  - reviewer
---

# Code Review Checklist

Use this checklist systematically when reviewing code changes. Do not skip sections.

## 1. Correctness

- [ ] Does the code do what the objective/design asked for?
- [ ] Are all edge cases handled? (empty inputs, null values, boundary conditions)
- [ ] Are error paths explicit and tested?
- [ ] Do types match at all interfaces?
- [ ] Are assertions/invariants maintained?

## 2. Security

- [ ] No hardcoded secrets, API keys, or credentials
- [ ] User input is validated/sanitized before use
- [ ] SQL queries use parameterized statements
- [ ] File paths are validated (no path traversal)
- [ ] Authentication/authorization checks are present where needed
- [ ] No sensitive data logged or exposed in error messages

## 3. Performance

- [ ] No N+1 query patterns
- [ ] No unnecessary allocations in hot paths
- [ ] Database queries use appropriate indexes
- [ ] Large collections are handled with pagination/streaming
- [ ] No blocking I/O in async contexts

## 4. Maintainability

- [ ] Code follows existing project conventions
- [ ] No unnecessary duplication (DRY)
- [ ] Functions are focused (single responsibility)
- [ ] Naming is clear and consistent
- [ ] Complex logic has explanatory comments

## 5. Testing

- [ ] New code has corresponding tests
- [ ] Tests cover happy path, error paths, and edge cases
- [ ] Tests are deterministic (no flakiness)
- [ ] Test assertions are specific (not just "doesn't crash")

## Severity Levels

- **CRITICAL**: Security vulnerability, data loss risk, or crash in production
- **HIGH**: Incorrect behavior, missing error handling, or broken functionality
- **MEDIUM**: Code smell, minor bug, or missing test coverage
- **LOW**: Style inconsistency, minor optimization opportunity
