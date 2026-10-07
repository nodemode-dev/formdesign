---
name: layout-director
description: "Runs the whole layout loop in formdes: intake, make, look, fix, deliver, with the gates between them. The entry point for any request to make or change a layout."
---

# Layout director

## Purpose

Takes a request in the person's words and returns a finished layout in formdes plus a short account of what is still theirs to supply. Owns the order of operations and the stopping rule.

## Preconditions

The formdes MCP server answers (`list_documents` works). If it does not, `./formdes mcp` starts the engine on first use; if tools are missing entirely, load `cli-fallback`.

## Procedure

1. **Collect material.** Gather absolute paths of everything the person pointed to (files, folders). Do not open or read them yourself: intake reads them more cheaply and returns a summary of a few kilobytes. If the person gave text in the chat (an ad idea, copy), put it in the brief.
2. **Write the brief.** Keep their words; add key: value lines for anything concrete they said (Headline, CTA, Date, Venue, Price, URL, Phone, Brand). Name the kind and size when they did ("square Instagram ad", "8-page A4 report"). `intake-curator` has the details.
3. **Intake.** `intake {paths, brief}`. Read the returned summary: title, articles and words, images with roles, detected kinds and fields, warnings, and the ranked templates.
4. **Choose.** Use the top ranked template unless the person named something else or the ranking's `why` shows it misread the kind; then `list_templates {category}` and pick by size and slots (`template-picker`).
5. **Make.** `make_layout {pack, template, fills?, brand?, variations?}`. Put brief facts that intake missed into `fills`. Ask for `variations: 2` or `3` when the person wants options.
6. **Look.** `preview {page: 1}`; preview other pages the report mentions (added pages, trimmed slots). Judge: is the headline right, are images cropped on their subjects, is sample text left where real content belongs, does the copy read.
7. **Fix.** `set_slots` for text and images by slot name; `run_commands` for anything structural (`layout-finisher`). Preview again after each meaningful change.
8. **Deliver.** `save_document` (a .fdes keeps the layout editable with its content pack) and `export_document` in the formats asked (`publisher`). Report in a few lines: the template used, pages, what is still sample text or placeholder art, and where the files are.

## Gate

Done when the four gates in the master SKILL.md hold. Never claim a layout is finished from the report alone: a preview must have been looked at after the last change.

## Hands back

The document open in formdes, saved and exported where asked, and a short summary naming anything still sample or placeholder.

## Owned resources

None of its own; it routes to the specialists and uses the shared `references/tools.md`, `references/templates.md`, `references/slots.md` and `references/brief.md`.
