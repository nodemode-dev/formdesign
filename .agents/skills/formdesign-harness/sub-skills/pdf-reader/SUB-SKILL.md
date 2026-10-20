---
name: pdf-reader
description: "Reads PDFs through formdes: structure-aware extraction of headings, paragraphs and lists, page text, search, page images, and turning a PDF into material for a new layout."
---

# PDF reader

## Purpose

Gets what is needed out of a PDF at the lowest token cost, either to answer a question about it or to lay it out again.

## Preconditions

An absolute path to the PDF.

## Procedure

1. **To re-lay it out, do not read it.** Pass the PDF to `intake` with the brief: intake extracts its structure (columns joined in reading order, running heads and page numbers dropped, hyphenation repaired, headings by size and weight, bullets as lists) and its images, and the Make pass uses them.
2. **To answer questions:** `read_pdf {path, mode: "info"}` for pages and metadata; `mode: "structure"` for the whole text as typed blocks (headings carry their page); `mode: "text", pages: [3, 4]` for exact page text; `mode: "search", query` for where a phrase occurs with context.
3. **To see a page:** `pdf_page_image {path, page, max_size}` (pages count from 1). Look before describing a chart, a form or a layout.

## Gate

The answer cites page numbers for what it quotes; a re-layout went through intake, not through pasted text.

## Hands back

Extracted text or blocks, page images, or a pack id from intake.

## Owned resources

None of its own.
