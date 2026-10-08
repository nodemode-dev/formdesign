# Editor

The window is a thin client in vanilla TypeScript and Vite. It draws pixels the layout core rendered, draws overlays from the core's geometry, and sends pointer and key events back; it never decides where text breaks or how a frame moves. It carries the form family's Studio Dark design language from formvis.

## Source

| Path | What lives there |
| --- | --- |
| `frontend/src/app.ts` | the composition root: the shell markup, mounting the Shell and the features |
| `frontend/src/engine/client.ts` | the socket to `/api/ws` (requests, binary frames, change events) and the `api()` REST helper |
| `frontend/src/editor/editor.ts` | the Editor: open documents, layout, selection, tool, `wantsText`, `exec`, `batch`, `inspect` |
| `frontend/src/editor/viewport.ts` | 512 px tiles cached per document and zoom level, damage invalidation, overlays, zoom and pan |
| `frontend/src/editor/input.ts` | pointer events coalesced to one request in flight, keys routed to shortcuts or the core |
| `frontend/src/ui/shell.ts`, `ui/workspace.ts` | the Shell: menus from the core's catalogue, the tool rail, document tabs, panels, dialogs, the status bar; the Workspace contract features use |
| `frontend/src/ui/panel-group.ts`, `dom.ts`, `menu.ts`, `dialogs.ts`, `themes.ts`, `icons.ts` | family controls vendored from formvis |
| `frontend/src/features/` | every panel, dialog and view, one module each, listed in `features/index.ts` |
| `frontend/src/styles/` | tokens, family sheets, and one sheet per feature |

## Features

| Module | Adds |
| --- | --- |
| `make.ts`, `make/` | the Make panel, Slots, the template gallery, brand kits |
| `properties.ts`, `properties/` | Properties, Character, Paragraph, Swatches, the Styles tab (paragraph, character, object), the control bar |
| `pages.ts`, `pages/` | Pages, Layers, Links |
| `document.ts`, `document/` | New Document, Document Setup, Export, Package, Preferences, Preflight, Info |
| `reader.ts`, `reader/` | the PDF Reader |
| `rulers.ts`, `glyphs.ts`, `tabs.ts`, `table.ts`, `book.ts`, `story.ts` | rulers and guides, Glyphs, Tabs, the Table panel, Books, the Story Editor (style column from `story.paras`, overset line) |
| `object-styles.ts`, `properties/hub.ts` | the Object view of the Styles tab: apply, New from selection, Options, Redefine, Delete over the core's `objectStyles` |
| `review.ts` | the Review group: Changes, Notes, Conditions |
| `separations.ts` | Separations Preview (plates and the ink limit as tile options) and Overprint Preview |
| `find.ts` | the Find/Change window over the core's `findChange`, saved queries over `/api/find/queries` |
| `spelling.ts` | Check Spelling, Dynamic Spelling (`spellMarks` drawn by the viewport) and the document dictionary |
| `pathtype.ts` | Type on a Path Options |
| `floating.ts` | drag and remembered position for floating windows |
| `document/preflight.ts` | the Preflight panel, its profiles and the Define dialog |

## How a feature plugs in

A feature exports `install(ws: Workspace)` and uses only the Workspace contract: `registerPanel` (id, label, group, column, create, onShow), `addCommands` (id, label, keys, enabled, checked, run), `addMenuItems`, `override` (replace a core command's default behaviour), `runCore`, `setControl`, `pickFiles`, `upload`, `onDropFiles`, `showPanel`. Commands with keys are resolved before the core's own shortcuts; while the core wants text (`wantsText`), single-key shortcuts go to the text instead.

## The zero-flash rules

These are binding; every UI change is checked against them under 4x CPU and network throttling (see [[Verification]]).

- The shell mounts once. Panels patch values in place (`setText`, `setValue`, `setClass`, `patchList` keyed by name); a container is never cleared to change one node.
- Space is reserved before content arrives: panels have fixed field heights, lists have minimum heights, the canvas keeps stale tiles until replacements are decoded.
- The theme is applied before first paint by the boot script in `frontend/index.html`; there is never a frame in the wrong theme.
- Every `z-index` is a `var(--z-*)` token from `frontend/src/styles/tokens.css`; overlays render into `#overlay-root`.
- Icons are inline `currentColor` SVG; there is no icon library.
- Colours are tokens; a feature sheet holds no raw colour values.

## Contract

- No layout logic in the window. If a feature needs to know where something is, it asks the core (`inspect`, `layout`, `overlays`, `slots`).
- New UI is a feature module registered in `features/index.ts`; the Shell is only edited for things every feature needs.
- A panel queries the core only while it is shown (`onShow`) and debounces on `selection` and `change` events.
- Vite stays on 5196 with `host: false`; the window never reaches the network except the engine.
- Read `frontend/HASHOUT_LOG.md` before editing.
