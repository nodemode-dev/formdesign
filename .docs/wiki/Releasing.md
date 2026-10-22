# Releasing

A release is a version bump on `main`, a `compile_v*.*.*` branch cut from it, and the draft release formcompiler (`nodemode-ai/formcompiler`) builds from that branch. FormDesign's own repository builds no release; its GitHub Actions only run the checks.

## Steps

1. Bump every version site in one commit, last in the series (the table is in `.docs/BRANCHING.md`).
2. Push a compile branch from it: `git push origin HEAD:compile_v0.7.0`. CI runs on it. The name must match the version in `src-tauri/tauri.conf.json`, or formcompiler refuses it and says why.
3. formcompiler finds the branch on its next watch run (every 8 hours), or at once when its watch is run by hand. Once CI has passed on that commit, it runs:

| Job | Runner | Produces |
| --- | --- | --- |
| `build` (macos-arm64) | macos-latest | `.dmg`, `.app.tar.gz` (renamed per platform), `formdesign-core-aarch64-apple-darwin` |
| `build` (linux-x64) | ubuntu-22.04 | `.deb`, `.rpm`, `.AppImage`, `formdesign-core-x86_64-unknown-linux-gnu` |
| `build` (windows-x64) | windows-latest | `.msi`, `-setup.exe`, `formdesign-core-x86_64-pc-windows-msvc.exe` |
| `core` | macos-latest | `formdesign-core-x86_64-apple-darwin` for Intel Macs |
| `release` | ubuntu-latest | `latest.json` when signatures exist, then a draft release `formdesign-v[version]` with every file on formcompiler's release page |

4. Download and try the bundles, then publish the draft on formcompiler. Publishing matters: `./formdesign install-core` on a machine without Rust downloads `formdesign-core-[triple]` from the published `formdesign-v[version]` release on formcompiler.

## Secrets

Set these in formcompiler's repository (Settings, Secrets and variables, Actions), never in FormDesign's. Each pair is optional; without it the bundle is built unsigned.

| Secret | Used for |
| --- | --- |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | signing the macOS app (base64 .p12, its password, the identity name) |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | notarising it (an app-specific password) |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | signing updater artifacts |

Empty secrets are unset before the build so Tauri does not try to import a blank certificate.

## The updater

The updater stays off until `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` holds a public key:

1. `npx tauri signer generate -w ~/.tauri/formdesign.key` (from `frontend/`), once. Keep the private key out of the repository.
2. Put the public key in `plugins.updater.pubkey` and the private key and its password in formcompiler's two `TAURI_SIGNING_*` secrets.
3. Release builds then pass `--config src-tauri/tauri.updater.conf.json`, sign the update archives, and the release job writes `latest.json`, which installed apps read from `https://github.com/nodemode-ai/formcompiler/releases/download/formdesign-updates/latest.json` once the release is published.

## Unsigned builds

An unsigned macOS app opens with right-click, Open the first time. An unsigned Windows installer shows SmartScreen; More info, Run anyway. Linux packages need nothing.
