---
name: publisher
description: "Saves and exports formdes documents: .fdes, PDF for print or screen, IDML for InDesign, HTML, EPUB, PNG and JPG images, text, packages, and books of several documents numbered and exported as one PDF."
---

# Publisher

## Purpose

Delivers the finished layout in the formats the person needs, at paths they can find, and reports them.

## Preconditions

The document to deliver is active (`list_documents`, then `switch_document {index}` if needed).

## Procedure

1. **Save the editable original.** `save_document {path: "/abs/folder/name.fdes"}`. A .fdes keeps the layout, its slots and the content pack it was made from, so it can be re-made or edited later. `.idml` hands it to InDesign; `.fdoc` is the core's native file.
2. **Export by purpose.**
   - Print: `export_document {format: "pdf", path}`; `options` can carry `pages` ("1-3,5"), and the exporter's print options (bleed and marks when the exporter supports them).
   - Screen and social: `png` or `jpg`, one page each: `options: {page: 0, scale: 2}` (page counts from 0); repeat per page.
   - Web: `html` (one self-contained page, styles inline, images embedded).
   - Reading: `epub` (reflowable) or `epub-fixed`.
   - Hand-off: `idml`; `txt`, `rtf` or `xml` for the text.
   - Print shop booklet: `booklet`.
3. **Package for a printer** with `run_commands [{"command": "file.package", "params": {"dir": "/abs/folder/name-package", "idml": true, "pdf": true}}]`: the document, its linked files and fonts list.
4. **Books, for work in parts** (chapters, a magazine's sections, a catalogue in several files): save each part as a .fdes, then `book {action: "create", name}`, `book {action: "add", book, doc}` for each part in order, `book {action: "source", book, index}` for the part whose styles lead, `book {action: "paginate", book}` and `book {action: "sync", book}`, then `book {action: "export", book, path: "/abs/folder/name.pdf"}` for one PDF. A part open with unsaved changes stops paginate and sync: save it first.
5. **Check** that each reported path exists and is not empty; for PDFs, `read_pdf {path, mode: "info"}` confirms the page count.

## Gate

Every requested file exists at the reported path with a plausible size, and the person has been told where.

## Hands back

The list of saved and exported paths.

## Owned resources

None of its own.
