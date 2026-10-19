---
name: publisher
description: "Saves and exports formdes documents: .fdes, PDF for print or screen, IDML, responsive multi-page websites and blogs, Publish Online links, HTML, EPUB, PNG and JPG images, text, packages, and books of several documents numbered and exported as one PDF."
---

# Publisher

## Purpose

Delivers the finished layout in the formats the person needs, at paths they can find, and reports them.

## Preconditions

The document to deliver is active (`list_documents`, then `switch_document {index}` if needed).

## Procedure

1. **Preflight against the destination.** `preflight {profile}` with the profile that matches where the file is going: `Commercial print` for a press (300 ppi, 9 pt of bleed, text inside the trim, CMYK), `PDF/X-1a, no transparency` when the printer asks for X-1a, `Saddle-stitched booklet` for stapled booklets (page count in fours), `Digital and screen` for PDFs, EPUB and the web. Fix the errors (missing bleed: extend the background frames past the trim; page count: add or remove pages) and tell the person about warnings you leave.
2. **Save the editable original.** `save_document {path: "/abs/folder/name.fdes"}`. A .fdes keeps the layout, its slots and the content pack it was made from, so it can be re-made or edited later. `.idml` writes an IDML package; `.fdoc` is the bare layout file, without slots or the content pack.
3. **Export by purpose.**
   - Print: `export_document {format: "pdf", path}`; `options` can carry `pages` ("1-3,5"), and the exporter's print options (bleed and marks when the exporter supports them).
   - Screen and social: `png` or `jpg`, one page each: `options: {page: 0, scale: 2}` (page counts from 0); repeat per page.
   - Website or blog: `export_document {format: "web", path: "/abs/folder/site"}` (a folder) or `path: ".../site.zip"`. It makes responsive pages in the document's own type and colours: `split` `auto` decides (an article per page for a magazine or report with several titles, a page per page for web templates, one page otherwise), or set `articles`, `pages` or `single`; `layout` comes from the template (`article`, `landing`, `card`, `email`, `docs`); `edition: true` adds the page-by-page edition (on by default for multi-page print layouts); `baseUrl` (where it will live) adds a sitemap, an RSS feed for articles and share links; `title`, `description`, `language` override what it reads from the document. The reply lists the pages made and `warnings`: fix them before calling it done (alt text in Object Export Options, a hyperlink on each button, placeholder art replaced).
   - A link to look at now: `publish_website {action: "publish", options}` serves the site from this computer and returns its `url`; publishing the same document again updates it; `list` and `unpublish {slug}` manage them. For the public web, export the folder or .zip and upload it to a static host.
   - One web page: `html` (self-contained, styles inline, images embedded).
   - Reading: `epub` (reflowable) or `epub-fixed`.
   - Hand-off: `idml`; `txt`, `rtf` or `xml` for the text.
   - Print shop booklet: `booklet`.
4. **Package for a printer** with `run_commands [{"command": "file.package", "params": {"dir": "/abs/folder/name-package", "idml": true, "pdf": true}}]`: the document, its linked files and fonts list.
5. **Books, for work in parts** (chapters, a magazine's sections, a catalogue in several files): save each part as a .fdes, then `book {action: "create", name}`, `book {action: "add", book, doc}` for each part in order, `book {action: "source", book, index}` for the part whose styles lead, `book {action: "paginate", book}` and `book {action: "sync", book}`, then `book {action: "export", book, path: "/abs/folder/name.pdf"}` for one PDF. A part open with unsaved changes stops paginate and sync: save it first.
6. **Check** that each reported path exists and is not empty; for PDFs, `read_pdf {path, mode: "info"}` confirms the page count; for a website, the folder has `index.html` and the reply's page list matches what the person expects.

## Gate

Every requested file exists at the reported path with a plausible size, and the person has been told where.

## Hands back

The list of saved and exported paths.

## Owned resources

None of its own.
