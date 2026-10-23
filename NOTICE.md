# Third-party notices

FormDesign is licensed under the Apache License 2.0 (see `LICENSE`). It includes or depends on the following works.

## Included in this repository

| Work | License | Where |
| --- | --- | --- |
| NodeMode stylized design system: tokens, theme profiles, theme boot, dom-stable helpers, icon subset, by way of formvis 1.65.0 | MIT (design system), Apache-2.0 (formvis) | `frontend/src/styles/`, `frontend/src/ui/themes.ts`, `frontend/src/ui/dom.ts`, `frontend/src/ui/icons.ts`, `frontend/index.html` |
| formvis 1.65.0 shell controls (menu, commands, toast, dialogs, dropdown, panel group), setup script layout, MCP stdio bridge pattern | Apache-2.0, Copyright 2026 nodemode-ai | `frontend/src/ui/`, `setup/` |
| CGATS001Compat-v2-micro.icc, a compact CMYK output profile from the Compact ICC Profiles collection by saucecontrol (https://github.com/saucecontrol/Compact-ICC-Profiles; 8464 bytes, creator signature `sawsctrl`, SHA-256 `73e1ba37d2bad5bab2a964f40a9eed96209666efc067c3322626214bbef234a0`) | CC0-1.0 (the collection states all its profiles are released to the public domain under CC0) | `backend/app/assets/`, the output intent `services/pdfpost.py` writes |

## Trademarks

FormDesign names no other application or its maker (see `CLAUDE.md`); formats are named by their extension. Product names a person may associate with those formats are the trademarks of their owners, and FormDesign is not affiliated with or endorsed by any of them. The font credits in this file name the fonts' publishers because their licences ask for it.

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
| hypher 0.1.8 (https://github.com/typst/hypher) | MIT OR Apache-2.0 (code); the embedded hyphenation patterns keep their own licences, read from each `patterns/hyph-*.tex` at tag v0.1.8: MIT for as, be, bn, cs, de, el, en, es, et, fi, fr, gu, hi, kn, la, lt, ml, mr, no, or, pa, pl, sq, sv, ta, te, tk; LPPL for af, ca, da, hr, is, it, ka, kmr, mn, nl, ru, sh, sl, tr, uk; BSD for bg, BSD-3-Clause for pt; the Unlicense for gl; a free-use grant for sa; hu used under MPL-1.1 (of MPL-1.1, GPL-2.0 or LGPL-2.1), its source in that file | hyphenation, compiled into the layout core unmodified. Slovak (`hyph-sk.tex`, GPL-2.0-or-later) is left out: `native/formdesign-text` turns hypher's default features off and lists every other language, so Slovak text is not hyphenated |
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
| Template font library: Anton, Archivo, Archivo Black, Barlow, Barlow Condensed, Bebas Neue, Cormorant Garamond, DM Sans, DM Serif Display, EB Garamond, Figtree, Fraunces, IBM Plex Mono, IBM Plex Sans, Instrument Serif, Inter Tight, Libre Baskerville, Manrope, Montserrat, Newsreader, Oswald, Outfit, Poppins, Space Grotesk, Space Mono, Syne, Work Sans (via @fontsource, Latin subsets converted from WOFF to TrueType by `setup/fonts.py`) | SIL OFL 1.1 | each family's licence is written to `backend/app/fonts/licenses/` at install |
| Template font library, full fonts: Abril Fatface, Josefin Sans, Lora, Merriweather, Playfair Display and Raleway, the upstream files unmodified from github.com/google/fonts at e7db6ca603fd4862010383ff17ffd6cc4ffac1c0 (`assets/fonts/full`, each folder with its OFL.txt), copied as they are by `setup/fonts.py` | SIL OFL 1.1, with Reserved Font Names | these families carry a Reserved Font Name, so a subset or a renamed copy may not keep the name; FormDesign ships the complete original files, and the layout core picks weights from their named instances |
| Libraries inside the Pillow wheel | libjpeg, libpng, libtiff, libwebp, libavif, OpenJPEG, Little CMS, HarfBuzz, Brotli, zstd, xz, libxcb: permissive (see the wheel's LICENSE); FreeType 2.14.3 used under the FreeType License | Portions of this software are copyright (c) 2026 The FreeType Project (www.freetype.org). All rights reserved. |
| PDFium inside pypdfium2 5.14.0 | BSD-3-Clause (the PDFium Authors, from code Foxit Software contributed under that licence), with Adobe's CMap data (BSD-3-Clause) and the third-party code PDFium compiles in (FreeType, Little CMS, OpenJPEG, libjpeg-turbo, libpng, libtiff, zlib, AGG 2.3), each under its own permissive licence | the list follows PDFium's `third_party` tree; licence to confirm against the notices of the pdfium-binaries build pypdfium2 5.14.0 ships |
| numpy 2.x wheel libraries: OpenBLAS (BSD-3-Clause), libgfortran and libquadmath | BSD-3-Clause; GPL-3.0 with the GCC Runtime Library Exception (libgfortran, libquadmath) | engine, through ezdxf |
| pypdf 5.9.0, httpx 0.28.1 (with httpcore), lxml | BSD-3-Clause | engine: PDF post-processing (`backend/app/services/pdfpost.py`), model provider requests, python-docx's XML |
| certifi | MPL-2.0 | engine: its `cacert.pem`, frozen unedited |
| Other Python packages frozen into the engine | MIT: annotated-doc, annotated-types, anyio, et-xmlfile, h11, httptools, jaraco.classes, jaraco.context, jaraco.functools, jeepney, mdurl, more-itertools, pydantic, pydantic-core, pyyaml, typing-inspection, watchfiles; MIT-0: cffi; BSD-3-Clause: click, idna, pycparser, python-dotenv, secretstorage, starlette, websockets; Apache-2.0: opentelemetry-api; MIT OR Apache-2.0: uvloop; Apache-2.0 OR BSD-3-Clause: cryptography (its wheel bundles OpenSSL 3, Apache-2.0); PSF-2.0: typing-extensions and the CPython runtime | read from the installed metadata of `backend/requirements.txt`, 2026-10-23 |
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

## Desktop shell: crates under other licences

Tauri links these crates unedited (no `[patch]` and no vendored copy):

| Crate | Licence | Why it is there |
| --- | --- | --- |
| cssparser, selectors, dtoa-short, option-ext | MPL-2.0 | Tauri's HTML handling (tauri-utils); used unmodified, source at crates.io |
| ring | Apache-2.0 AND ISC | TLS for the updater (rustls) |
| webpki-root-certs | CDLA-Permissive-2.0 | root certificates for the updater's TLS |
| r-efi | MIT OR Apache-2.0 OR LGPL-2.1-or-later | used under MIT |

The Linux AppImage also carries the build machine's WebKitGTK, GTK and GLib (LGPL-2.1-or-later) as separate shared libraries. The installers carry this file and `LICENSE`.

## Build tools, not distributed

| Package | License | Notes |
| --- | --- | --- |
| TypeScript, Vite | Apache-2.0, MIT | frontend build |
| @tauri-apps/cli | Apache-2.0 or MIT | desktop bundles |
| PyInstaller | GPL-2.0 with the bootloader exception | freezes the engine; the exception covers the produced binaries |
| Rust toolchain, cargo | MIT or Apache-2.0 | builds formdesign-core and the shell |

No icons, images, presets, templates or artwork from other layout applications are included. Other applications' file formats are named only to describe compatibility.
