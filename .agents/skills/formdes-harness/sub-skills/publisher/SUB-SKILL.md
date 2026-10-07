---
name: publisher
description: "Saves and exports formdes documents: .fdes, PDF for print or screen, IDML for InDesign, HTML, EPUB, PNG and JPG images, text, and packages."
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
4. **Check** that each reported path exists and is not empty; for PDFs, `read_pdf {path, mode: "info"}` confirms the page count.

## Gate

Every requested file exists at the reported path with a plausible size, and the person has been told where.

## Hands back

The list of saved and exported paths.

## Owned resources

None of its own.
