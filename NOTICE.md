# Third-party notices

formdes is licensed under the Apache License 2.0 (see `LICENSE`). It includes or depends on the following works.

## Included in this repository

| Work | License | Where |
| --- | --- | --- |
| NodeMode stylized design system: tokens, theme profiles, theme boot, dom-stable helpers, icon subset, by way of formvis 1.65.0 | MIT (design system), Apache-2.0 (formvis) | `frontend/src/styles/`, `frontend/src/ui/themes.ts`, `frontend/src/ui/dom.ts`, `frontend/src/ui/icons.ts`, `frontend/index.html` |
| formvis 1.65.0 shell controls (menu, commands, toast, dialogs, dropdown, panel group, dock), setup script layout, MCP stdio bridge pattern | Apache-2.0, Copyright 2026 nodemode-ai | `frontend/src/ui/`, `setup/` |

## Built into the layout core

| Package | License | Notes |
| --- | --- | --- |
| Source Sans 3, Source Serif 4, Inter, JetBrains Mono (embedded in formdes-core) | SIL OFL 1.1 | |
| vello_cpu, kurbo, skrifa, harfrust, krilla, hayro, image, resvg, quick-xml, zip and their dependencies | MIT, Apache-2.0, BSD, ISC, Zlib | per `cargo metadata` |
| fancy-regex 0.19 (with regex-automata, regex-syntax, bit-set) | MIT (fancy-regex), MIT OR Apache-2.0 (the rest) | GREP in Find/Change: lookahead, lookbehind and backreferences |

## Distributed with the app

| Package | License | Notes |
| --- | --- | --- |
| Inter, JetBrains Mono (via @fontsource) | SIL OFL 1.1 fonts, MIT packaging | interface fonts |
| Template font library: Abril Fatface, Anton, Archivo, Archivo Black, Barlow, Barlow Condensed, Bebas Neue, Cormorant Garamond, DM Sans, DM Serif Display, EB Garamond, Figtree, Fraunces, IBM Plex Mono, IBM Plex Sans, Instrument Serif, Inter Tight, Josefin Sans, Libre Baskerville, Lora, Manrope, Merriweather, Montserrat, Newsreader, Oswald, Outfit, Playfair Display, Poppins, Raleway, Space Grotesk, Space Mono, Syne, Work Sans (via @fontsource, Latin subsets converted from WOFF to TrueType by `setup/fonts.py`) | SIL OFL 1.1 | each family's licence is written to `backend/app/fonts/licenses/` at install |
| @tauri-apps/api, tauri | Apache-2.0 or MIT | desktop shell |
| fastapi, uvicorn, python-multipart | MIT, BSD-3-Clause, Apache-2.0 | engine |
| Pillow | MIT-CMU | engine, image intake |
| pypdfium2 (with PDFium) | Apache-2.0 or BSD-3-Clause (PDFium: BSD-3-Clause and Apache-2.0) | engine, the PDF reader and extractor |
| python-docx, openpyxl | MIT | engine, Word and Excel intake |
| markdown-it-py, mdit-py-plugins | MIT | engine, Markdown intake |
| striprtf | BSD-3-Clause | engine, RTF intake |
| fonttools, brotli | MIT | font library build and font inspection |
| keyring | MIT | API keys for optional model providers in the system's password store |

## Build tools, not distributed

| Package | License | Notes |
| --- | --- | --- |
| TypeScript, Vite | Apache-2.0, MIT | frontend build |
| @tauri-apps/cli | Apache-2.0 or MIT | desktop bundles |
| PyInstaller | GPL-2.0 with the bootloader exception | freezes the engine; the exception covers the produced binaries |
| Rust toolchain, cargo | MIT or Apache-2.0 | builds formdes-core and the shell |

No Adobe icons, images, presets, templates or artwork are included. InDesign is named only to describe compatibility.
