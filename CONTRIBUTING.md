# Contributing to Grove

Thank you for your interest in contributing to Grove! This document provides guidelines for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch from `main`
4. Make your changes
5. Submit a pull request

## Development Setup

```bash
# Ensure Rust 1.85+ is installed
rustup update

# Bootstrap the project
./scripts/bootstrap.sh

# Build
cargo build

# Run tests
cargo test
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Follow existing code patterns and conventions

## Pull Request Process

1. Update documentation if your change affects public APIs or behavior
2. Add tests for new functionality
3. Ensure all tests pass: `cargo test`
4. Ensure code compiles without warnings: `cargo clippy`
5. Use clear, descriptive commit messages following [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat: add new feature`
   - `fix: resolve bug`
   - `docs: update documentation`
   - `test: add tests`
   - `refactor: restructure code`

## Reporting Issues

- Use GitHub Issues to report bugs
- Include steps to reproduce the issue
- Include your OS, Rust version, and Git version
- Include relevant log output

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
