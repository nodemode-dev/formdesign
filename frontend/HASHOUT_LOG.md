# HASHOUT_LOG · frontend · formdes

Living log for the window. Source files stay comment-free; what they would have said lives here, indexed. Read it before editing `frontend/`.

Current build version: v0.8.0
Last updated: 2026-10-08 · Separations, the Review group, Object Style Options, the Story Editor's style column, function keys while typing, the Make bar in a short group

---

## Org-navi-map

```
frontend/
├── index.html                    critical style, the pre-paint theme boot, stylesheets in order
├── package.json                  Vite, TypeScript; @fontsource families (dev) feed the template font library
├── vite.config.ts                5196, host false, strictPort; /api (with sockets) and /mcp to 8196
├── tsconfig.json                 strict, bundler resolution
├── oc_install(.bat)              npm install
├── boot_frontend(.bat)           npm run dev
├── HASHOUT_LOG.md                this log
├── public/
│   ├── favicon.svg, favicon.ico, apple-touch-icon.png   the formdes mark
│   └── splash.html               the desktop splash while the engine starts
└── src/
    ├── main.ts                   entry
    ├── app.ts                    composition root: the SHELL markup, home screen, engine status
    ├── version.ts                VERSION
    ├── engine/
    │   └── client.ts             socket RPC to the engine, binary frames, change events, REST helper
    ├── editor/
    │   ├── editor.ts             the Editor singleton: docs, layout, selection, tools, exec, inspect
    │   ├── viewport.ts           512 px tiles, damage invalidation, overlays, zoom and pan
    │   └── input.ts              pointer and key routing to the core (wantsText from each pointer reply)
    ├── features/                 one module per feature, registered through ui/workspace.ts (index.ts lists them)
    │   ├── make*, properties*, pages*, document*, reader*   Make, Properties and text panels, Pages, Layers, Links, dialogs, Reader
    │   ├── properties/hub.ts     the Styles tab: paragraph, character and object styles behind one switch
    │   ├── rulers.ts             canvas rulers and drag-out guides (guide.add)
    │   ├── glyphs.ts, tabs.ts    the Glyphs and Tabs panels (fonts load from /api/core/font)
    │   ├── table.ts              the Table panel
    │   ├── object-styles.ts      the Styles tab's object view: apply, New, Options, Redefine, Delete (core objectStyles)
    │   ├── review.ts             the Review group: Changes, Notes, Conditions
    │   ├── separations.ts        Separations Preview and the ink limit (tile options), Overprint Preview
    │   ├── book.ts               the Book window over /api/books
    │   └── story.ts              the Story Editor (story.replaceRange of the changed span only; style column from story.paras)
    ├── styles/
    │   ├── tokens.css            family tokens (vendored, verbatim from formvis 1.65.0)
    │   ├── shell.css, panels.css, dialogs.css, light-mode.css, scrollbars.css, fonts.css   family sheets (vendored)
    │   └── formdes.css           formdes's own rules, tokens only
    └── ui/
        ├── dom.ts, themes.ts     zero-flash helpers and the 14 profiles (vendored)
        ├── icons.ts              inline currentColor SVG set (vendored, grows with formdes's own)
        ├── menu.ts, commands.ts, toast.ts, dialogs.ts, dropdown.ts, panel-group.ts   family controls (vendored; panel-group adds startMin and border-aware tab fitting)
        ├── shell.ts, workspace.ts   the Shell implements the Workspace contract features install into
        ├── tool-icons.ts         the tool rail glyphs
        ├── brand.ts              the formdes mark
        └── home.ts               the start screen
```

---

## Table of contents

- [index.html](#index)
- [src/app.ts](#app)
- [src/engine/client.ts](#client)
- [src/ui (vendored family files)](#vendored)
- [src/ui/brand.ts](#brand)
- [src/ui/home.ts](#home)
- [src/ui/shell.ts](#shell)
- [src/features (0.8.0 panels)](#panels080)
- [src/styles/make.css](#make-css)

---

## index.html {#index}

### index.html · theme boot
- refers-to: frontend/index.html :: inline script in head
- kind: rationale
- logged: v0.1.0 · 2026-10-07

Verbatim from formvis 1.65.0 (stylized-nodemode theme-boot.js, appearance generation 3, default profile `studio`, accent `#f97316`): the persisted profile and accent land on the root before the body paints, which is the whole mechanism behind no theme flash on reload. formdes shares the storage key `nodemode_ui_settings` with the family on purpose, so a theme chosen in formvis carries over. `formdes.css` loads after the family sheets and before `scrollbars.css`, which stays last.

---

## src/app.ts {#app}

### app.ts · SHELL
- refers-to: frontend/src/app.ts :: SHELL
- kind: explainer
- logged: v0.1.0 · 2026-10-07

The shell is formvis's grid (title bar, options row, rail, stage column with document tabs, a properties column, a resizer and a dock column, status bar) mounted once. In formdes the options row is InDesign's Control panel, the rail holds the core's tool catalog, and the stage carries three canvases: `fd-tiles` (pixels from the core), `fd-hilite` (text highlight, blended as difference so selected type inverts as in InDesign) and `fd-overlay` (selection, guides, tool feedback).

---

## src/engine/client.ts {#client}

### client.ts · EngineClient
- refers-to: frontend/src/engine/client.ts :: EngineClient
- kind: explainer
- logged: v0.1.0 · 2026-10-07

One socket to `/api/ws`. Requests get increasing ids and resolve when their reply arrives; binary replies are split from their 4-byte length and JSON header. `changed` events (any source, agents included) and `hello` update `rev` and `selRev` and reach listeners. A closed socket rejects everything pending and reconnects with backoff from 250 ms to 4 s.

---

## src/ui (vendored family files) {#vendored}

### dom.ts, themes.ts, icons.ts, menu.ts, commands.ts, toast.ts, dialogs.ts, dropdown.ts, panel-group.ts, dock.ts · provenance
- refers-to: frontend/src/ui :: the files named
- kind: rationale
- logged: v0.1.0 · 2026-10-07

Copied verbatim from formvis 1.65.0 (Apache-2.0, the same owner; dom.ts, themes.ts and part of icons.ts are the NodeMode stylized design system, MIT). Their in-file comments are kept as they are upstream so a later family update can be diffed and taken whole; formdes's own files carry none. formdes adds icons by appending to `ICONS`, never by editing the family's glyphs.

---

## src/ui/brand.ts {#brand}

### brand.ts · MARK
- refers-to: frontend/src/ui/brand.ts :: MARK
- kind: explainer
- logged: v0.1.0 · 2026-10-07

The formdes mark in the family's construction (formvis's tile, rim and bloom, and its blue to violet to pink to amber paint gradient): an open spread, an image block and a headline bar in the paint gradient, text lines on paper. Ids are namespaced per copy so two marks on screen never share gradients. `assets/formdes_icon.png`, the favicons and the splash are renders of this same SVG.

---

## src/ui/home.ts {#home}

### home.ts · HomeScreen
- refers-to: frontend/src/ui/home.ts :: HomeScreen
- kind: explainer
- logged: v0.1.0 · 2026-10-07

formvis's start screen recipe (the first action white as the call to action, the rest dark, shortcut key caps, a drop hint, recent documents below). formdes leads with Make a layout, then New document and Open. It shows with no document open, or when forced from the brand button; Esc leaves it while a document is open.

---

## src/ui/shell.ts {#shell}

### shell.ts · onKeyCommand
- refers-to: frontend/src/ui/shell.ts :: Shell.onKeyCommand
- kind: explainer
- logged: v0.8.0 · 2026-10-08

While the core wants text, a combo without Mod goes to the text so single letters type rather than switch tools. Function keys (F1 to F12, with any modifiers) are exempt because they never produce text; before 0.8.0 the Type tool swallowed F11, Shift+F11, Shift+F6 and Shift+F9. Escape is exempt so it can leave text editing.

---

## src/features (0.8.0 panels) {#panels080}

### separations.ts · SeparationsPanel
- refers-to: frontend/src/features/separations.ts :: SeparationsPanel
- kind: explainer
- logged: v0.8.0 · 2026-10-08

Modes are Off, Separations and Ink Limit. The panel only sets `viewport.opts.plate` and `viewport.opts.inkLimit` and invalidates the tile cache; formdes-core renders the plate (0 to 3) as greys, or sums the four plates and paints red where the total passes the limit. The limit is clamped to 100 to 400. Overprint Preview is a separate tile option so it can be on in any mode; the View menu's checked state reads the same option.

### review.ts · ChangesPanel, NotesPanel, ConditionsPanel
- refers-to: frontend/src/features/review.ts
- kind: explainer
- logged: v0.8.0 · 2026-10-08

Three panels in the side column's Review group, each a keyed `patchList` over the core's lists (`changes.list`, `note.list`, `condition.list`) reloaded on document change. Track changes reads its checkbox from `editor.doc.trackChanges`, which formdes-core adds to each entry of the document list, because `inspect` settings do not carry it. Accept and reject address a change by story and start offset.

### object-styles.ts · ObjectStylesPanel
- refers-to: frontend/src/features/object-styles.ts :: ObjectStylesPanel
- kind: explainer
- logged: v0.8.0 · 2026-10-08

The buttons act on the picked row, or on the selection's applied style when nothing is picked; the three bracketed built-in styles never enable Options, Redefine or Delete. Options sends one `objectStyles` edit with every field, where null means Not set; the core re-applies the style to every object that uses it, so the canvas changes without a separate apply.

---

## src/styles/make.css {#make-css}

### make.css · narrow layout and the sticky bar
- refers-to: frontend/src/styles/make.css :: .mk-layout, .mk-col-out, .mk-go
- kind: explainer
- logged: v0.8.0 · 2026-10-08

A sticky element cannot rise above the top of its containing block. In the narrow dock layout the output column started below the brief, so in a short Make group the Make layout bar was cut off until scrolled. The output column is `display: contents` there, making `.mk-layout` the bar's containing block; the column separator is the input column's bottom border, and `.mk-layout` carries the column gap. The container query restores the column as a flex box for the wide two-column layout.
