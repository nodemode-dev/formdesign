# formdes MCP tools

Generated from the engine's catalogue by `./formdes agents sync`; do not edit by hand.

## Contents

- Intake and make: `intake`, `list_templates`, `get_template`, `make_layout`
- Look and fix: `preview`, `get_slots`, `set_slots`, `get_document`, `preflight`, `find_change`
- Documents: `list_documents`, `switch_document`, `new_document`, `open_document`, `save_document`, `export_document`, `book`
- Any command: `run_commands`, `find_commands`, `undo`, `redo`
- PDF sources: `read_pdf`, `pdf_page_image`
- Brands and templates: `list_brands`, `save_brand`, `validate_template`, `save_template`, `library`
- Other: `publish_website`, `review`, `place_word`, `outline`, `inspect`, `look`, `use_tool`, `show`, `list_commands`, `run_command`

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

Opens a .fdes, .fdoc or .idml file. A .pdf, or a .pub, .qxd or .qxp (format versions 3.1 to 4.1), .pmd or .p65 file, is rebuilt as an editable layout (pages at the source size, text frames with fonts, sizes and colours, placed pictures, rectangles and lines; what cannot be converted stays as a locked backdrop layer) and a fidelity report comes back; backdrop: auto or never. A .dxf drawing (model space; ASCII or binary) becomes one page sized to its extents with lines, polylines, arcs, circles, ellipses, splines and solid hatches as shapes and text as text frames; skipped entities are counted in the report. .dwg cannot be read (no permissively licensed reader): export a DXF. .indd cannot be read: export an IDML package from the application that made it.

- `path`: string; required; Absolute path (~ is expanded).
- `backdrop`: string (auto | never)

## save_document

Saves the active document (.fdes by default, keeps slots and the content pack; .fdoc or .idml by extension).

- `path`: string; Absolute path (~ is expanded).

Note: writes or replaces files.

## export_document

Exports the active document: pdf, idml, html, epub, epub-fixed, txt, rtf, xml, png or jpg (one page: options.page from 0), a print booklet, or web: a responsive multi-page website (path is a folder, or a .zip). web options: split auto|articles|pages|single, layout article|landing|card|email|docs, edition (true adds the page-by-page edition), baseUrl (adds sitemap, feed and share links), title, description, language. Returns the pages made and warnings to fix.

- `format`: string (pdf | idml | html | web | epub | epub-fixed | txt | rtf | xml | png | jpg | booklet); required
- `path`: string; required; Absolute path (~ is expanded).
- `options`: object; Passed to the exporter (pdf: pages, bleed, marks...; web: split, layout, edition, baseUrl, title, description, language).

Note: writes or replaces files.

## publish_website

Publish Online: the active document as a website that formdes serves on this computer at a link (127.0.0.1), kept up to date by publishing again. publish (options as export_document's web options; publishing the same document again updates its site). list: the published sites with their links. unpublish (slug). To put a site on the public web, export_document web to a folder or .zip and upload that to a static host.

- `action`: string (publish | list | unpublish); required
- `slug`: string
- `options`: object

Note: writes or replaces files.

## review

Review the active document. import_pdf_comments (path of a PDF this document exported, annotated in Acrobat or Preview): comments, highlights, strike-outs and insert-text marks become Notes at the text under them, returning what was imported and what could not be placed. share: publishes a review site (Share for Review) on this computer and returns its link; reviewers click a page to pin a comment, and each arrives as a Note at that position. comments: the shared review's comments with replies and state. reply (id, text, name) and resolve (id, resolved) answer them. stop: takes the review site down.

- `action`: string (import_pdf_comments | share | comments | reply | resolve | stop); required
- `path`: string; Absolute path (~ is expanded).
- `id`: integer
- `text`: string
- `name`: string
- `resolved`: boolean; default true

Note: writes or replaces files.

## place_word

Places a .docx with Microsoft Word Import Options. scan (path): the Word paragraph and character styles in the file (with counts), the document's styles and the mapping used last time. place (path): styleMap {paragraph: {'Heading 1': {mode: existing|new|word, name}}, character: {...}} maps each Word style to a document style, a new style or keeps Word's (use word); options removeStyles, preserveOverrides, footnotes, quotes (booleans). Places into frame (or the selected frame), otherwise loads the Place cursor. The mapping is remembered per document.

- `action`: string (scan | place); required
- `path`: string; required; Absolute path (~ is expanded).
- `styleMap`: object
- `options`: object
- `frame`: integer

Note: writes or replaces files.

## run_commands

Runs layout commands on the active document as one undoable batch (all or nothing): [{command, params}]. Later steps can use earlier results as $N.id. find_commands lists commands and their parameters.

- `commands`: array; required

## find_commands

Searches the layout commands by id, label or menu, or lists a domain (type, layout, object, table, swatch, style, file...): one line each with its parameters, and how many match. With no match it suggests close ids and lists the domains.

- `query`: string
- `domain`: string; Command id prefix, e.g. table or swatch.
- `limit`: integer; default 20

Note: read-only.

## undo

Steps back through the active document's history.

- `steps`: integer; default 1

## redo

Steps forward through the active document's history.

- `steps`: integer; default 1

## preflight

Checks the active document for output problems: overset text, missing fonts or links, low resolution images, RGB in print. profile names a preflight profile ([Basic], Commercial print, PDF/X-1a no transparency, Saddle-stitched booklet, Digital and screen, or one the person saved) that adds bleed, trim safety, minimum type size, maximum resolution, spot ink count, transparency and page count checks.

- `min_ppi`: integer
- `profile`: string

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

## book

Books collect saved .fdes documents as chapters. list: the books and the saved documents that can be added. create (name). info, add (doc path, at), remove (index), move (index, to), source (index: the style source chapter), paginate (number pages across chapters), sync (copy the source's styles and swatches to every chapter), export (one PDF of every chapter, written to path or the Exports folder). Chapters open with unsaved changes stop paginate and sync.

- `action`: string (list | create | info | add | remove | move | source | paginate | sync | export); required
- `book`: string; The .fdbook path (from list or create).
- `name`: string
- `doc`: string; Absolute path (~ is expanded).
- `at`: integer
- `index`: integer
- `to`: integer
- `path`: string; Absolute path (~ is expanded).

Note: writes or replaces files.

## find_change

Find/Change across the active document. mode text (plain words; ^p return, ^t tab, ^n line break, ^9 digit, ^$ letter, ^? any character), grep (regular expressions with \r return, \n line break, ~= en dash, ~_ em dash, ~e ellipsis, ~{ ~} curly quotes, lookahead and lookbehind; $1 in change), glyph (glyph {char: a character or U+2014, fontFamily, fontStyle} to toGlyph) or object (object {kind: text|graphic|shape|line|group, fill, stroke, strokeWeight, opacity 0 to 1, layer, objectStyle} to objectChange). findFormat and changeFormat take paragraphStyle, characterStyle and character attributes (fontFamily, fontStyle, size, fill). op count, all (matches with page and context), change_all (one undo step; locked text is skipped), queries (the saved and built-in queries). query runs a saved query by name; save_as saves these settings. An empty change with no changeFormat deletes the matches.

- `op`: string (count | all | change_all | queries); required
- `mode`: string (text | grep | glyph | object); default "text"
- `find`: string
- `change`: string
- `query`: string
- `save_as`: string
- `caseSensitive`: boolean
- `wholeWord`: boolean
- `scope`: string (document | story | selection); default "document"
- `include`: object; locked, hidden, parents: booleans (default false)
- `findFormat`: object
- `changeFormat`: object
- `glyph`: object
- `toGlyph`: object
- `object`: object
- `objectChange`: object

Note: writes or replaces files.

## library

Object libraries: reusable objects (a logo lockup, a footer, a pull-quote box) saved with their styles, swatches and images. list: the libraries. create (name). items (name): each item's index, name and description. add (name, ids or the selection, title, description). place (name, index, x and y in points on the spread, spread): a copy in the active document, bringing missing styles and swatches. rename (name, index, title, description). remove (name, index).

- `action`: string (list | create | items | add | place | rename | remove); required
- `name`: string
- `index`: integer
- `ids`: array
- `title`: string
- `description`: string
- `x`: number
- `y`: number
- `spread`: integer

Note: writes or replaces files.

## outline

A compact outline of the active document: each spread with its pages and parent, and every item on it in one line (id, kind, slot, story, bounds in points, overset, the first words of its text), plus styles in use. Cheaper than get_document when you need to find things; page limits it to one page (from 1).

- `page`: integer
- `max_items`: integer; default 120

Note: read-only.

## inspect

Properties of objects by id: kind, bounds, fill, stroke, object style, layer, story, columns, overset, locked and hidden. fields keeps only the keys you name, so replies stay short.

- `ids`: array; required
- `fields`: array

Note: read-only.

## look

Looks at a page: a PNG (a region of it when region is given, in page points [x0, y0, x1, y1]) plus a short list of visible problems on that page: overset text, empty graphic frames, objects off the page.

- `page`: integer; default 1
- `region`: array
- `max_size`: integer; default 900

Note: read-only.

## use_tool

Uses one of the 36 layout tools the way a person would, for what only a tool does (drawing a path with the pen, a polygon, a gradient drag): tool id (selection, type, pen, rectangleFrame, ellipse, polygon, gradientSwatch...), then a click [x, y] or a drag [x0, y0, x1, y1] in the same spread points outline and inspect use, optionally text to type afterwards. One undo step per gesture.

- `tool`: string; required
- `spread`: integer; default 0
- `click`: array
- `drag`: array
- `text`: string

## show

Moves the person's FormDesign window so they can follow what you did: go to a page, zoom (fit page, fit spread or a percentage), open a panel, pick a tool, or select objects by id. Changes nothing in the document.

- `page`: integer; Page to show, counting from 1.
- `zoom`: string; fit-page, fit-spread, or a percentage such as 150.
- `panel`: string; Panel id: pages, layers, links, properties, character, paragraph, swatches, styles, preflight, make, slots, terminal, connect, find...
- `tool`: string; Tool id from the engine's tool list, e.g. selection, type, rectangle-frame.
- `select`: array; Object ids to select (get_document lists them).

## list_commands

The one command catalogue the menus, the search box, the CLI and agents share: engine commands and the window's own commands, each with id, label, menu, group, what it does, when to use it, its parameters, whether it changes the document and whether Undo can reverse it. q filters by word; group filters by group (file, edit, layout, type, object, path, table, style, swatch, window...).

- `q`: string
- `group`: string
- `limit`: integer; default 60

Note: read-only.

## run_command

Runs one command from list_commands by id, including the window's own commands (panels, view, workspaces), with its parameters. Commands Undo cannot reverse wait for the person's approval. For several layout steps as one undo step, run_commands is cheaper.

- `id`: string; required
- `params`: object
