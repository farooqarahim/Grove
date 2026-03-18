---
name: rust-patterns
description: Rust-specific coding patterns and conventions used in the Grove codebase. Loaded for Builder and Reviewer agents working on Rust code.
applies_to:
  - builder
  - reviewer
---

# Grove Rust Patterns

## Error Handling

- Use `GroveResult<T>` (alias for `Result<T, GroveError>`) everywhere
- Use `?` operator for propagation — no `.unwrap()` in production code
- Map external errors: `external_call().map_err(|e| GroveError::External(e.to_string()))?`
- Use `GroveError::NotFound` for missing resources, `GroveError::Config` for config issues

## Database (rusqlite)

- All DB operations through `rusqlite::Connection`
- Use `conn.execute()` for writes, `conn.query_row()` for single reads
- Use `conn.prepare()` + `stmt.query_map()` for multi-row reads
- Always use parameterized queries: `[&run_id]`, never string interpolation
- Migrations in `crates/grove-core/src/db/migrations/` — numbered sequentially

## Serialization

- `#[derive(Serialize, Deserialize)]` on all IPC types
- `#[serde(rename_all = "snake_case")]` for enums
- `#[serde(default)]` for optional fields that may be missing in older data

## File I/O

- Use `std::fs` for sync file operations
- Always create parent directories: `fs::create_dir_all(parent)?`
- Use `Path` and `PathBuf` — never string concatenation for paths

## Tauri Commands

- All commands are `#[tauri::command]` async functions
- Use `spawn_blocking` for sync operations (DB, git, file I/O)
- Return `Result<T, String>` where `T: Serialize`
- Error conversion: `.map_err(|e| e.to_string())?`

## Testing

- `#[cfg(test)] mod tests` at bottom of each module
- Use `tempfile::tempdir()` for filesystem tests
- Use in-memory SQLite: `Connection::open_in_memory()`
- Test names: `snake_case_describing_behavior`
