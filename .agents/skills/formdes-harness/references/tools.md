# formdes MCP tools

Generated from the engine's catalogue by `./formdes agents sync`; do not edit by hand.

## Contents

- Intake and make: `intake`, `list_templates`, `get_template`, `make_layout`
- Look and fix: `preview`, `get_slots`, `set_slots`, `get_document`, `preflight`
- Documents: `list_documents`, `switch_document`, `new_document`, `open_document`, `save_document`, `export_document`
- Any command: `run_commands`, `find_commands`, `undo`, `redo`
- PDF sources: `read_pdf`, `pdf_page_image`
- Brands and templates: `list_brands`, `save_brand`, `validate_template`, `save_template`

## intake

Reads files (md, docx, pdf, txt, rtf, html, csv, xlsx, images, gifs, folders) and a brief into a content pack: articles as typed blocks, images with orientation, palette and focal point, brief fields. Returns a compact summary and the top templates for it.

- `paths`: array; Absolute paths of files or folders.
- `brief`: string; What the person wants, in their words; key: value lines (headline, cta, date, venue, price, url) are read as fields.
- `kind`: string; Force the kind (magazine, ad-social, web-landing, flyer...).

## list_templates

Lists templates (built-in and the person's): id, category, size, pages, kinds. Filter by category (magazine, newsletter, book, report, print, stationery, ad, web, presentation), kind or a query.

- `category`: string
- `kind`: string
- `query`: string

Note: read-only.

## get_template

One template's card and slots (what make_layout and set_slots can fill).

- `id`: string; required

Note: read-only.

## make_layout

Makes a new document from a template filled with a content pack: text into slots, the main article threaded through the body flow with continuation pages added as needed, images matched to frames by shape and role, copy fitted. Returns a short report of what is filled, kept as sample, placeholder, trimmed and unused. Without template, the best ranked one is used.

- `pack`: string; required; Pack id from intake.
- `template`: string
- `brand`: any; A saved brand name, or {name, palette: {accent, accent2, ink, paper, dark, light, muted, tint}, fonts: {display: [family, style], text, accent}, logo}.
- `fills`: object; Slot values that win over the pack: {cta: 'Book now', title: '...'}.
- `title`: string
- `variations`: integer; default 1; Also make alternatives in the next best templates.
- `placeholders`: boolean; default true; Fill image frames that have no picture with on-palette sample art.

## preview

The active document's page as a PNG image (pages count from 1).

- `page`: integer; default 1
- `max_size`: integer; default 1100

Note: read-only.

## get_slots

The active document's named slots (frames labelled slot:NAME and flow:NAME): kind, page, the first words of their text or the placed image.

- no parameters

Note: read-only.

## set_slots

Sets slots in the active document: text (a string, or a list for several paragraphs), an image (a pack image id like img2, or an absolute path), or markdown for flow:body. Reports slots that now overflow.

- `values`: object; required; {slot: value}

## get_document

The active document: title, size, pages, every page's items (id, kind, slot, bounds in points, overflow), styles and swatches. detail items lists items; summary only counts.

- `detail`: string (summary | items); default "items"

Note: read-only.

## list_documents

The open documents and which is active.

- no parameters

Note: read-only.

## switch_document

Makes an open document active, by its index from list_documents.

- `index`: integer; required

## new_document

Opens a new document: from a template (with its sample content, or empty slots with sample false) or blank with a size in points.

- `template`: string
- `brand`: any
- `sample`: boolean; default true
- `title`: string
- `width`: number
- `height`: number
- `pages`: integer
- `facing`: boolean
- `preset`: string; Letter, A4, A5, Tabloid...

## open_document

Opens a .fdes, .fdoc or .idml file.

- `path`: string; required; Absolute path (~ is expanded).

## save_document

Saves the active document (.fdes by default, keeps slots and the content pack; .fdoc or .idml by extension).

- `path`: string; Absolute path (~ is expanded).

Note: writes or replaces files.

## export_document

Exports the active document: pdf, idml, html, epub, epub-fixed, txt, rtf, xml, png or jpg (one page: options.page from 0), or a print booklet.

- `format`: string (pdf | idml | html | epub | epub-fixed | txt | rtf | xml | png | jpg | booklet); required
- `path`: string; required; Absolute path (~ is expanded).
- `options`: object; Passed to the exporter (pdf: pages, bleed, marks...).

Note: writes or replaces files.

## run_commands

Runs layout commands on the active document as one undoable batch (all or nothing): [{command, params}]. Later steps can use earlier results as $N.id. find_commands lists commands and their parameters.

- `commands`: array; required

## find_commands

Searches the 432 layout commands by id, label or menu: each with its parameters.

- `query`: string; required
- `limit`: integer; default 20

Note: read-only.

## undo

Steps back through the active document's history.

- `steps`: integer; default 1

## redo

Steps forward through the active document's history.

- `steps`: integer; default 1

## preflight

Checks the active document for output problems: overset text, missing fonts or links, low resolution images, RGB in print.

- `min_ppi`: integer; default 150

Note: read-only.

## read_pdf

Reads a PDF: info (pages, metadata), structure (headings, paragraphs, lists as markdown-like blocks, columns joined, headers removed), text of pages, or search.

- `path`: string; required; Absolute path (~ is expanded).
- `mode`: string (info | structure | text | search); default "structure"
- `pages`: array; 1-based pages for text.
- `query`: string

Note: read-only.

## pdf_page_image

A PDF page as a PNG image (pages count from 1).

- `path`: string; required; Absolute path (~ is expanded).
- `page`: integer; default 1
- `max_size`: integer; default 1100

Note: read-only.

## list_brands

The saved brand kits: palette, fonts and logo that re-skin any template.

- no parameters

Note: read-only.

## save_brand

Saves a brand kit: name, palette roles (#rrggbb for ink, paper, accent, accent2, muted, tint, dark, light), font roles ([family, style] for display, text, accent, mono), logo (absolute path).

- `name`: string; required
- `palette`: object
- `fonts`: object
- `logo`: string

## validate_template

Checks a template JSON spec (docs/TEMPLATES.md): structural problems, then compiles it with sample content and returns pages, slots, overflow, preflight and page images.

- `spec`: object; required
- `render`: boolean; default true

Note: read-only.

## save_template

Saves a template JSON spec to the person's template library after it compiles; it appears in list_templates and the gallery.

- `spec`: object; required
