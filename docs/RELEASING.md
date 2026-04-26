# Releasing Grove

This runbook documents how Grove releases are produced, signed, and distributed
across macOS, Linux, and Windows. The pipeline is implemented in:

- `.github/workflows/release.yml` &mdash; CLI archives (`grove`)
- `.github/workflows/release-desktop.yml` &mdash; Tauri desktop bundles
- `.github/workflows/ci.yml` &mdash; pre-merge checks
- `.github/workflows/security-audit.yml` &mdash; weekly RustSec + cargo-deny

---

## Channels and tag conventions

Tags drive everything. There is no separate "release branch."

| Tag pattern | Channel | Manifest | GitHub release |
|---|---|---|---|
| `vX.Y.Z` | stable | `latest.json` | latest |
| `vX.Y.Z-beta.N` | beta | `beta.json` | pre-release |
| `vX.Y.Z-rc.N` | beta | `beta.json` | pre-release |
| `vX.Y.Z-alpha.N` | beta | `beta.json` | pre-release |

The semver pre-release segment is what selects the channel; everything else is
treated as stable. Tag validation is enforced inside the workflows.

The desktop app's `tauri.conf.json` updater endpoint currently points at
`latest.json` (stable channel). To ship a beta-channel build, override the
endpoint at build time (see "Beta channel" below) and distribute that build to
opt-in users.

---

## Required secrets

These secrets must exist on the **private** repo (where the workflows run).

For a public production desktop release, configure the updater signing key and
the platform signing credentials for the platforms you distribute. Builds
without Apple notarization or Windows code signing are suitable for internal or
beta testing only, because users will see operating-system trust warnings.

### Always required

| Secret | Purpose |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | minisign private key used to sign updater manifests. Without this, auto-update is disabled. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the minisign key (omit if unset). |
| `RELEASES_REPO_TOKEN` | Fine-grained PAT with `Contents: write` on `Grove-Tools/grove-loom`. Used by `release-desktop.yml` to publish to the public update repo. |

Generate the minisign keypair once:

```bash
npx @tauri-apps/cli signer generate -w ~/.tauri/grove.key
# Put the PRIVATE key contents into TAURI_SIGNING_PRIVATE_KEY.
# Put the PUBLIC key into tauri.conf.json -> plugins.updater.pubkey.
```

### Optional &mdash; macOS code signing + notarization

When all of the following are present, macOS bundles are signed and notarized.
When any are missing, the build still succeeds but produces an *unsigned* DMG
that triggers Gatekeeper warnings.

| Secret | Notes |
|---|---|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` of your Developer ID Application cert. |
| `APPLE_CERTIFICATE_PASSWORD` | Password for the `.p12`. |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Grove Inc (ABCD123456)`. |
| `APPLE_ID` | Apple ID email used for notarization. |
| `APPLE_PASSWORD` | App-specific password (NOT your Apple ID password). |
| `APPLE_TEAM_ID` | 10-char Apple developer team ID. |

Encode the cert:

```bash
base64 -i developer_id.p12 | pbcopy   # paste into APPLE_CERTIFICATE
```

### Optional &mdash; Windows code signing

| Secret | Notes |
|---|---|
| `WINDOWS_CERTIFICATE` | Base64-encoded `.pfx`. |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the `.pfx`. |

When unset, installers are produced without a signature and trigger SmartScreen
warnings. The workflow detects the secret, imports the cert at build time, and
sets `WINDOWS_CERTIFICATE_THUMBPRINT` for `tauri-action` to consume.

---

## Cutting a release

```bash
# 1. Bump version everywhere it's pinned
#    - Cargo.toml [workspace.package].version
#    - crates/grove-gui/src-tauri/tauri.conf.json -> "version"
#    - CHANGELOG.md (top entry)
#
# 2. Commit and tag
git add -A
git commit -m "chore: release v0.2.0"
git tag -s v0.2.0 -m "Grove 0.2.0"
git push origin main --tags
```

Both `release.yml` and `release-desktop.yml` fire on the tag push. The CLI
release publishes to **this repo**; the desktop release publishes to
**`Grove-Tools/grove-loom`** so unauthenticated updaters can pull from it.

For a beta:

```bash
git tag -s v0.2.0-beta.1 -m "Grove 0.2.0-beta.1"
git push origin v0.2.0-beta.1
```

Or trigger manually via the Actions UI ("Run workflow" -> tag input).

---

## Beta channel consumption

The default updater feed is `latest.json`. Beta tags publish a separate
`beta.json` to the same release. To opt a build into the beta channel, override
the updater endpoint at build time:

```bash
# One-time beta build
jq '.plugins.updater.endpoints = ["https://github.com/Grove-Tools/grove-loom/releases/latest/download/beta.json"]' \
  crates/grove-gui/src-tauri/tauri.conf.json > /tmp/tauri.beta.json
mv /tmp/tauri.beta.json crates/grove-gui/src-tauri/tauri.conf.json
```

Then run the release. Stable users on `latest.json` will not see the beta;
beta users on `beta.json` will see *both* feeds (stable releases also generate a
`latest.json`, but the beta build is pinned to `beta.json`, so they only see
beta tags).

A future improvement is to make the channel runtime-selectable via an in-app
toggle that swaps endpoints &mdash; tracked separately.

---

## What the desktop pipeline produces

For each tag, every platform contributes:

| Platform | Bundle | Updater archive |
|---|---|---|
| macOS aarch64 | `*_aarch64.dmg` | `*.app.tar.gz` (+ `.sig`) |
| macOS x86_64  | `*_x64.dmg`     | `*.app.tar.gz` (+ `.sig`) |
| Linux x86_64  | `*.deb`, `*.AppImage` | `*.AppImage.tar.gz` (+ `.sig`) |
| Windows x86_64 | `*-setup.exe` (NSIS), `*.msi` | `*-setup.nsis.zip` (+ `.sig`) |

Plus, once per release:

- `latest.json` or `beta.json` &mdash; updater manifest (one entry per platform)
- `SHA256SUMS` &mdash; sorted SHA-256 of every shipped artifact
- `grove-X.Y.Z.cdx.json` &mdash; CycloneDX SBOM
- SLSA build provenance attestation (verifiable via `gh attestation verify`)

---

## Verifying a release

```bash
# Checksums
shasum -a 256 -c SHA256SUMS

# Build provenance (once GitHub propagates the attestation)
gh attestation verify Grove_0.2.0_aarch64.dmg \
  --repo <owner>/<private-repo>

# Updater signature (any platform)
# Tauri's updater verifies this automatically using pubkey from tauri.conf.json.
# Manual check:
minisign -V -P "<base64 pubkey>" -m Grove_0.2.0.app.tar.gz
```

---

## Wix upgrade code (Windows MSI)

`tauri.conf.json -> bundle.windows.wix.upgradeCode` is the **MSI upgrade code**
&mdash; a UUID that must remain identical across all future MSI releases of the
same product. **Never change it once shipped.** If you change it, Windows will
treat new installs as a different product and refuse to upgrade existing
installations.

If you have not yet shipped a signed Windows release, generate a fresh UUID and
pin it before the first stable Windows release:

```bash
uuidgen | tr '[:lower:]' '[:upper:]'
```

Replace the placeholder UUID in `tauri.conf.json` and commit.

---

## Troubleshooting

### `TAURI_SIGNING_PRIVATE_KEY is not set`

The desktop workflow refuses to build without an updater signing key. Either:

1. Add the secret (see above), or
2. Disable updater artifacts by removing `createUpdaterArtifacts` from
   `tauri.conf.json` &mdash; **but this kills auto-update.**

### macOS bundle says "Apple cannot check it for malicious software"

You shipped without `APPLE_CERTIFICATE` / `APPLE_SIGNING_IDENTITY`. The bundle
is unsigned. Add the secrets and re-tag.

### `cargo deny` fails on a new crate

Either:

1. Add the crate's license to `deny.toml -> licenses.allow`, or
2. Add a per-crate exception under `licenses.exceptions`, or
3. Replace the dependency.

Do not lower the confidence threshold or weaken `unknown-registry`/`unknown-git`.

### Updater manifest ends up missing a platform

The publish job logs a warning per missing signature/asset. Most common
causes:

- Build for that platform failed (check the `build` matrix logs)
- Bundle naming changed in a Tauri upgrade &mdash; update the glob patterns in
  `release-desktop.yml -> Generate updater manifest` step

### Cache thrashing in CI

`Swatinem/rust-cache` keys on the toolchain version + workspace fingerprint.
If you bump `RUST_TOOLCHAIN` in the workflow, the cache invalidates once and
re-fills.

---

## Things this pipeline deliberately does NOT do

- **Homebrew tap / cask publishing** &mdash; can be added as a follow-up job that
  consumes the macOS DMG + SHA256SUMS.
- **Linux .rpm packaging** &mdash; Tauri 2 does not produce `.rpm` natively; if
  needed, add a downstream job that converts `.deb` via `alien` or rebuilds
  with `rpmbuild`.
- **Reproducible builds** &mdash; not enforced. Possible follow-up via
  `SOURCE_DATE_EPOCH` and a deterministic build profile.
- **Sigstore / cosign signatures** &mdash; SLSA provenance is attested, but
  binaries are not separately cosign-signed. Considered overlap with provenance.
