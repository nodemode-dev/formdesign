# Releasing

A release is a version bump on `main`, a `v*.*.*` tag, and the draft GitHub release the desktop workflow builds from that tag.

## Steps

1. Bump every version site in one commit, last in the series (the table is in `.docs/BRANCHING.md`).
2. Tag and push: `git tag -a v0.7.0 -m "formdes v0.7.0: ..."`, then `git push origin main --tags`.
3. `.github/workflows/desktop.yml` runs:

| Job | Runner | Produces |
| --- | --- | --- |
| `build` (macos-arm64) | macos-latest | `.dmg`, `.app.tar.gz` (renamed per platform), `formdes-core-aarch64-apple-darwin` |
| `build` (linux-x64) | ubuntu-22.04 | `.deb`, `.rpm`, `.AppImage`, `formdes-core-x86_64-unknown-linux-gnu` |
| `build` (windows-x64) | windows-latest | `.msi`, `-setup.exe`, `formdes-core-x86_64-pc-windows-msvc.exe` |
| `core` | macos-latest | `formdes-core-x86_64-apple-darwin` for Intel Macs |
| `release` | ubuntu-latest | `latest.json` when signatures exist, then a draft release with every file |

4. Download and try the bundles, then publish the draft. Publishing matters: `./formdes install-core` on a machine without Rust downloads `formdes-core-[triple]` from the published `v[version]` release.

## Secrets

Set these under Settings, Secrets and variables, Actions. Each pair is optional; without it the bundle is built unsigned.

| Secret | Used for |
| --- | --- |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | signing the macOS app (base64 .p12, its password, the identity name) |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | notarising it (an app-specific password) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | signing updater artifacts |

Empty secrets are unset before the build so Tauri does not try to import a blank certificate.

## The updater

The updater stays off until `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` holds a public key:

1. `npx tauri signer generate -w ~/.tauri/formdes.key` (from `frontend/`), once. Keep the private key out of the repository.
2. Put the public key in `plugins.updater.pubkey` and the private key and its password in the two `TAURI_SIGNING_*` secrets.
3. Release builds then pass `--config src-tauri/tauri.updater.conf.json`, sign the update archives, and the release job writes `latest.json`, which installed apps read from `https://github.com/nodemode-ai/formdesign/releases/latest/download/latest.json`.

## Unsigned builds

An unsigned macOS app opens with right-click, Open the first time. An unsigned Windows installer shows SmartScreen; More info, Run anyway. Linux packages need nothing.
