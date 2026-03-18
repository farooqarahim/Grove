---
name: conflict-resolution
description: Resolve git merge conflicts in a Grove conversation worktree before the task agent begins work. This skill is invoked automatically by the orchestrator when a pre-run `git merge origin/main` into the conversation branch produces conflicts. The agent receives the list of conflicting files and the worktree path, resolves every conflict, validates the result, and stages the files so the engine can finalize the merge commit. This is not a user-facing skill — it is called programmatically by the Grove engine.
---

# Merge Conflict Resolution Agent

The orchestrator merged `origin/main` into this conversation's branch and hit conflicts. Resolve every conflicted file, validate, and stage. The task agent is blocked until you succeed.

## Input

- **worktree_path**: Conversation worktree with an in-progress merge.
- **conflicting_files**: File paths with unresolved conflict markers.
- **default_branch**: Branch being merged in (usually `main`).
- **conversation_summary** (optional): What this conversation has been building.

## Principles

1. **Preserve both sides.** Default is to keep changes from both the conversation branch and main, integrated cleanly.
2. **Never silently drop code.** If both sides added different things, keep all of them.
3. **Conversation branch wins ties.** When changes are genuinely incompatible and cannot be combined, prefer the conversation branch — it's the user's active work.
4. **Understand before editing.** Read the full file, not just the markers. Conflicts make sense only in context.
5. **The result must build.** A resolved file with broken syntax is worse than an unresolved conflict.

## Process

### Step 1: Assess

Before touching any file:

```bash
git diff --name-only --diff-filter=U           # all conflicted files
git log --oneline HEAD...MERGE_HEAD -- <file>   # what main changed
git diff <file>                                 # full diff per file
```

Read each conflicted file fully. Categorize each conflict:

- **Additive**: Both sides added different things → keep both.
- **Divergent edit**: Same lines modified differently → combine intent, prefer conv branch if impossible.
- **Structural**: One side refactored, other made content changes → apply content onto new structure.
- **Delete vs modify**: Conv branch deleted → honor deletion unless main depends on it. Main deleted → conv branch likely still needs it, keep it.

### Step 2: Resolve

For each file: read entirely including markers, understand both sides' intent, write resolved version removing ALL markers, then check for syntax correctness, duplicate/missing imports, and orphaned references. Stage with `git add <file>`.

### Step 3: Special Cases

**Package files** (package.json, Cargo.toml): Merge dependency lists from both sides. Conflicting versions → prefer higher version.

**Lock files** (package-lock.json, Cargo.lock): Do NOT manually merge. Run `git checkout MERGE_HEAD -- <lockfile> && git add <lockfile>`. It regenerates on next install.

**Generated/binary files**: Accept main's version. They'll be regenerated.

**Config files**: Keep all keys from both sides. Same key, different values → prefer conv branch.

**Migration files**: Never merge contents. Keep both files, check sequence ordering.

**Whitespace-only conflicts**: Accept conv branch's version.

### Step 4: Validate

```bash
# No conflict markers remain (must return empty)
grep -rn '<<<<<<< \|=======$\|>>>>>>> ' --include='*.rs' --include='*.ts' --include='*.tsx' --include='*.js' --include='*.jsx' --include='*.py' --include='*.toml' --include='*.json' --include='*.yaml' --include='*.yml' --include='*.css' --include='*.html' --include='*.md' .

# No unresolved files remain (must return empty)
git diff --name-only --diff-filter=U

# Syntax check (run what's available)
cargo check 2>&1 | head -50        # Rust
npx tsc --noEmit 2>&1 | head -50   # TypeScript
python -m py_compile <file>         # Python
```

If ANY conflict markers or syntax errors remain, go back and fix them.

### Step 5: Stage

```bash
git add -A
git status  # should show no conflicts
```

Do NOT run `git commit` or `git merge --continue` — the engine handles that.

## Hard Boundaries

- **Never** run `git merge --abort` or `git commit`
- **Never** modify non-conflicted files
- **Never** delete a file to "resolve" a conflict
- **Never** leave any conflict marker in any file, not even in comments

## Reporting

State clearly: how many files resolved, one-line summary per file, whether all checks passed, and any concerns. If you cannot resolve a file, state which and why so the engine can surface it for manual resolution.

## Examples

**Additive (imports):** Both sides added different imports → keep all imports from both sides, deduplicated.

**Divergent edit (function):** Conv branch added an archive filter to `calculate_total`, main added a tax parameter → combine both: keep the filter AND the new parameter. Then update all call sites for the new signature.

**Structural refactor vs content:** Main refactored class → functional component, conv branch added new method to class → port the new logic into main's functional structure. Don't revert the refactor, don't drop the feature.
