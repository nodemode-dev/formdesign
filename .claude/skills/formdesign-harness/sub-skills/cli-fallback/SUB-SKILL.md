---
name: cli-fallback
description: "Uses the FormDesign command line when the MCP server is not connected: make, intake, templates, export and check-template from a shell in the formdesign folder."
---

# CLI fallback

## Purpose

Produces the same layouts without MCP, from a shell, and hands back file paths.

## Preconditions

A shell in the formdesign repository folder after `./formdesign install` (Windows: `formdesign.bat`). The CLI runs its own layout core: the window does not need to be open.

## Procedure

1. List templates: `./formdesign templates` (filter with `--category ad`, `--kind web-landing`, `--q menu`; `--json` for machine output; `./formdesign templates ID --json` for one).
2. Inspect material: `./formdesign make` is usually enough, but `./formdesign intake FILES --brief "..." --json` prints the pack summary and ranking first.
3. Make and export in one command:
   `./formdesign make article.md photos/ logo.png --brief "A warm 6-page feature for Northlight. CTA: Subscribe" --template magazine-feature-spread --out out/feature.fdes --out out/feature.pdf --json`
   Options: `--brand NAME`, `--title`, `--fills '{"cta": "Book now"}'`, `--variations 3` (writes numbered files), `--no-placeholders`. `--out` takes .fdes, .pdf, .idml, .html, .epub, .png or .jpg (one image per page) and may repeat. Relative paths resolve from where the command runs.
4. Export an existing document: `./formdesign export doc.fdes --out doc.pdf --out doc.idml`.
5. The printed report is the same as make_layout's: read `kept`, `placeholders` and `fit`, then re-run with `--fills` to finish.

## Gate

The command exits 0 and the `saved` list in its output names every requested file.

## Hands back

Paths of the written files and the report.

## Owned resources

None of its own.
