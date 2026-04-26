# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- Public documentation set covering installation, configuration, CLI usage, integrations, workflows, GUI usage, daemon operation, troubleshooting, and releases.
- Multi-platform release workflows for CLI archives and Tauri desktop bundles.
- Release artifacts include checksums, CycloneDX SBOMs, and GitHub build provenance attestations.

### Security

- CI and scheduled security checks run `cargo-audit` and `cargo-deny`.
- Desktop updater artifacts require Tauri updater signatures before publication.
