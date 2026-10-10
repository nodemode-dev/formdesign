# Project Format

formdes saves two kinds of file: `.fdes` documents and `.fdbook` books. Both are designed to be opened by other tools: a `.fdes` is a zip whose layout is formdes's own `.fdoc` format, and a `.fdbook` is plain JSON.

## .fdes documents

A zip archive. The first entry is stored uncompressed so a file sniffer can recognise it.

| Entry | Compression | What it holds |
| --- | --- | --- |
| `mimetype` | stored | `application/vnd.formdes+zip` |
| `formdes.json` | deflated | formdes's metadata (below) |
| `document.fdoc` | stored | the layout itself, as the core serialises it (formdes-model's zip: `mimetype`, `doc.json`, assets): pages, spreads, parents, stories, styles, swatches, links |
| `content/pack.json` | deflated | the content pack the layout was made from, when it was made by the Make pass |
| `thumbs/page-1.png` | stored | a 512 px thumbnail of the first page for file browsers and the home screen |

`formdes.json`:

| Field | Meaning |
| --- | --- |
| `format`, `version` | `"formdes"`, `1` |
| `app` | the formdes version that wrote it |
| `title` | the document title |
| `template`, `brand` | the template id and brand kit it was made with, or null |
| `brief` | the brief text it was made from |
| `pack` | the content pack id |
| `slots` | slot and flow names (`title`, `flow:body`) to the ids of the frames that hold them when the layout was made |
| `created`, `modified` | seconds since the epoch |

Opening a `.fdes` sends `document.fdoc` to the core with `file.openBytes`, named `<stem>.fdoc`, and keeps the rest as the document's metadata. Saving rebuilds the zip (`serialize_fdes` in `backend/app/engine/documents.py`). A bare `.fdoc` file opens and saves too (Save As to `.fdoc` writes one), without formdes's metadata; `.idml` opens and exports through the core.

Documents saved by formdes before 0.17.0 hold the previous engine's document instead of `document.fdoc` and no longer open; the error asks for an IDML export of the document instead.

## .fdbook books

JSON, kept under `~/Formdes/Projects/Books/`:

```json
{
  "format": "formdes-book",
  "version": 1,
  "documents": ["/home/me/Formdes/Projects/Chapter 1.fdes", "/home/me/Formdes/Projects/Chapter 2.fdes"],
  "styleSource": 0
}
```

`documents` are absolute paths in chapter order; `styleSource` is the index of the chapter whose styles and swatches Synchronize Styles copies to the others. Numbering, synchronising and exporting copy each chapter's `document.fdoc` into a temporary core book, run the core's book command there, and write changed documents back into their `.fdes` packages, keeping every other entry (`backend/app/services/books.py`).

## File associations

The desktop app registers `.fdes` (formdes Document, `application/vnd.formdes+zip`) and `.idml` (`application/vnd.adobe.indesign-idml-package`) in `src-tauri/tauri.conf.json`.

## Contract

- Readers ignore entries they do not know; writers keep entries they do not change.
- A change to `formdes.json` adds fields; renaming or removing one bumps `version` and keeps reading the old one.
- The layout is always the core's own serialisation (formdes-model's `.fdoc`), never a re-encoding, so the core and other tools can open it directly.
