# Rust Release Pipeline: 0.3.0-alpha

Date: 2026-02-11
Status: active

## Scope

Rust-first release pipeline for:
- `forge`
- `forged`
- `forge-agent-runner`
- `fmail`

Archive naming contract:
- `forge_<version-no-v>_<os>_<arch>.tar.gz`

## Build script

Script: `scripts/release-build-artifacts.sh`

Inputs:
- `--version <vX.Y.Z[-prerelease]>`
- `--os <linux|darwin>`
- `--arch <amd64|arm64>`
- optional `--out-dir <dir>`

Behavior:
- builds Rust release binaries with embedded metadata (`FORGE_VERSION`, `FORGE_COMMIT`, `FORGE_BUILD_DATE`)
- stages binary names expected by installers (`forge`, `forged`, `forge-agent-runner`, `fmail`)
- packs tarball in release naming format

## CI/CD changes

- `ci.yml` build job now runs Rust artifact build script (snapshot-style prerelease version string).
- `release.yml` now builds artifacts via matrix per OS/arch and publishes tarballs + checksums.
- release prerelease flag auto-enabled when tag contains `-`.

## Homebrew automation

`release.yml` Homebrew update job now consumes Rust tarballs from matrix artifacts.
Expected files:
- `dist/forge_<version>_darwin_arm64.tar.gz`
- `dist/forge_<version>_darwin_amd64.tar.gz`

Checksums are injected into formula update PR as before.

## Tag format

Allowed:
- `v0.3.0`
- `v0.3.0-alpha.1`

## Local smoke

```bash
scripts/release-build-artifacts.sh \
  --version v0.3.0-alpha.local \
  --os darwin \
  --arch arm64 \
  --out-dir /tmp/forge-dist

tar -tzf /tmp/forge-dist/forge_0.3.0-alpha.local_darwin_arm64.tar.gz
```

Expected tar entries:
- `forge`
- `forged`
- `forge-agent-runner`
- `fmail`
