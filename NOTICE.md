# Third-party notices

FormDesign is licensed under the Apache License 2.0 (see `LICENSE`). It includes or depends on the following works.

## Included in this repository

| Work | License | Where |
| --- | --- | --- |
| NodeMode stylized design system: tokens, theme profiles, theme boot, dom-stable helpers, icon subset, by way of formvis 1.65.0 | MIT (design system), Apache-2.0 (formvis) | `frontend/src/styles/`, `frontend/src/ui/themes.ts`, `frontend/src/ui/dom.ts`, `frontend/src/ui/icons.ts`, `frontend/index.html` |
| formvis 1.65.0 shell controls (menu, commands, toast, dialogs, dropdown, panel group), setup script layout, MCP stdio bridge pattern | Apache-2.0, Copyright 2026 nodemode-ai | `frontend/src/ui/`, `setup/` |

## Built into the layout core

| Package | License | Notes |
| --- | --- | --- |
| Source Serif 4, Source Sans 3 (Adobe, https://github.com/adobe-fonts), Inter (https://github.com/rsms/inter), JetBrains Mono (https://github.com/JetBrains/JetBrainsMono) | SIL OFL 1.1 | embedded in `formdesign-core`; each licence is in `native/formdesign-core/fonts/` |
| SCOWL en_US Hunspell dictionary (Kevin Atkinson and the word-list authors it credits) | permissive SCOWL licence, with the Ispell, UKACD and WordNet notices it carries | embedded in `formdesign-core` for US English spelling; the full notice is `native/formdesign-core/dict/en_US.COPYRIGHT.txt` |
| fancy-regex 0.19 (with regex-automata, regex-syntax, bit-set) | MIT (fancy-regex), MIT OR Apache-2.0 (the rest) | GREP in Find/Change: lookahead, lookbehind and backreferences |
| spellbook 0.4 (helix-editor/spellbook, https://github.com/helix-editor/spellbook), with foldhash and hashbrown | MPL-2.0 (spellbook, used unmodified; its source is available at that address), Zlib (foldhash), MIT OR Apache-2.0 (hashbrown) | Hunspell spelling in every language |

## Built into FormDesign's own engine

| Package | License | Notes |
| --- | --- | --- |
| fontdb, harfrust, skrifa (read-fonts, font-types) | MIT (fontdb, harfrust), MIT OR Apache-2.0 (skrifa) | font discovery, OpenType shaping and font metrics in `native/formdesign-text` |
| unicode-linebreak, unicode-bidi, unicode-segmentation | Apache-2.0 (unicode-linebreak), MIT OR Apache-2.0 (the rest) | line-break opportunities (UAX 14) and bidirectional text (UAX 9) |
| vello_cpu, kurbo, peniko, krilla, hayro, image and their dependencies | MIT OR Apache-2.0 (vello_cpu, kurbo, peniko, krilla, hayro, image), MIT, BSD, Zlib (dependencies, per `cargo metadata`) | page rendering, PDF writing, placed PDFs and image decoding in `native/formdesign-render` |
| hypher 0.1 (https://github.com/typst/hypher) | MIT OR Apache-2.0 (code); the embedded hyphenation patterns keep their own licences (LPPL, MPL, MIT, BSD-3-Clause, per pattern file in the crate's source) | hyphenation; the pattern licences are still under review |
| quick-xml, zip | MIT, MIT | IDML and EPUB in `native/formdesign-interchange` |
| i_overlay 2.2 (with i_float, i_shape, i_tree, i_key_sort; https://github.com/iShape-Rust/iOverlay) | MIT | Pathfinder: union, difference, intersection and exclusion of shapes in `native/formdesign-engine` |
| qrcodegen 1.8 (Project Nayuki, https://www.nayuki.io/page/qr-code-generator-library) | MIT | QR code encoding in `native/formdesign-engine` |
| moxcms 0.8 (https://github.com/awxkee/moxcms) | BSD-3-Clause OR Apache-2.0 (used under Apache-2.0) | ICC colour transforms for soft proofing in `native/formdesign-engine` |
| xterm.js 6.0.0 and @xterm/addon-fit 0.11.0 (https://github.com/xtermjs/xterm.js) | MIT, Copyright (c) 2017-2019 The xterm.js authors, 2014-2016 SourceLair Private Company, 2012-2013 Christopher Jeffrey | the Terminal panel in `frontend/src/features/terminal.ts` |
| pywinpty 3.0.5 (https://github.com/andfoy/pywinpty), Windows only | MIT | ConPTY sessions for the Terminal panel in `backend/app/services/pty.py` |

## Downloaded at your request, not distributed

| Work | License | Notes |
| --- | --- | --- |
| Hunspell dictionaries from the LibreOffice dictionaries repository (https://github.com/LibreOffice/dictionaries) | each its own (GPL, LGPL, MPL, BSD or Creative Commons, listed in the dictionary's folder there) | fetched by Edit > Language Dictionaries > Get into your data folder; none ships with FormDesign |

## Distributed with the app

| Package | License | Notes |
| --- | --- | --- |
| Inter, JetBrains Mono (via @fontsource) | SIL OFL 1.1 fonts, MIT packaging | interface fonts |
| Template font library: Abril Fatface, Anton, Archivo, Archivo Black, Barlow, Barlow Condensed, Bebas Neue, Cormorant Garamond, DM Sans, DM Serif Display, EB Garamond, Figtree, Fraunces, IBM Plex Mono, IBM Plex Sans, Instrument Serif, Inter Tight, Josefin Sans, Libre Baskerville, Lora, Manrope, Merriweather, Montserrat, Newsreader, Oswald, Outfit, Playfair Display, Poppins, Raleway, Space Grotesk, Space Mono, Syne, Work Sans (via @fontsource, Latin subsets converted from WOFF to TrueType by `setup/fonts.py`) | SIL OFL 1.1 | each family's licence is written to `backend/app/fonts/licenses/` at install |
| @tauri-apps/api, tauri | Apache-2.0 or MIT | desktop shell |
| fastapi, uvicorn, python-multipart | MIT, BSD-3-Clause, Apache-2.0 | engine |
| Pillow | MIT-CMU | engine, image intake |
| pypdfium2 (with PDFium) | Apache-2.0 or BSD-3-Clause (PDFium: BSD-3-Clause and Apache-2.0) | engine, reading PDFs into content packs |
| python-docx, openpyxl | MIT | engine, Word and Excel intake |
| ezdxf (with numpy, pyparsing) | MIT (numpy: BSD-3-Clause, pyparsing: MIT) | engine, reading DXF drawings |
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
| Rust toolchain, cargo | MIT or Apache-2.0 | builds formdesign-core and the shell |

No icons, images, presets, templates or artwork from other layout applications are included. Other applications' file formats are named only to describe compatibility.
