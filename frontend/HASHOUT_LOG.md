# HASHOUT_LOG · frontend · formdes

Living log for the window. Source files stay comment-free; what they would have said lives here, indexed. Read it before editing `frontend/`.

Current build version: v0.18.0
Last updated: 2026-10-10 · copy, cut and paste reach the system clipboard

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
    │   ├── make*, properties*, pages*, document*   Make, Properties and text panels, Pages, Layers, Links, dialogs
    │   ├── properties/hub.ts     the Styles tab: paragraph, character and object styles behind one switch
    │   ├── rulers.ts             canvas rulers and drag-out guides (guide.add)
    │   ├── glyphs.ts, tabs.ts    the Glyphs and Tabs panels (fonts load from /api/core/font)
    │   ├── table.ts              the Table panel
    │   ├── object-styles.ts      the Styles tab's object view: apply, New, Options, Redefine, Delete (core objectStyles)
    │   ├── review.ts             the Review group: Changes, Notes, Conditions
    │   ├── separations.ts        Separations Preview and the ink limit (tile options), Overprint Preview
    │   ├── find.ts               the Find/Change window (findChange, /api/find/queries)
    │   ├── spelling.ts           Check Spelling, Dynamic Spelling, the document dictionary
    │   ├── pathtype.ts           Type on a Path Options (pathType, type.pathOptions)
    │   ├── floating.ts           drag and remembered position for floating windows
    │   ├── interactive.ts        Object States, Buttons and Forms (media options), Page Transitions
    │   ├── xml.ts                the XML Structure and Tags panels
    │   ├── library.ts            the Library panel over /api/libraries, drag onto the page
    │   ├── workspaces.ts         Window > Workspace and the title-bar switcher (/api/prefs/workspaces)
    │   ├── shortcuts.ts          Edit > Keyboard Shortcuts: sets, tool commands, Show Set (/api/prefs/shortcuts)
    │   ├── proof.ts              View > Proof Setup and Proof Colors (the tiles' proof option)
    │   ├── dictionaries.ts       Edit > Language Dictionaries (/api/dictionaries)
    │   ├── book.ts               the Book window over /api/books
    │   ├── clipboard.ts          the system clipboard around edit.copy, cut, paste and Paste without Formatting
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
        ├── dock.ts               the two panel columns: order, hidden groups, heights, width, groups made from dropped tabs
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
- [src/features (0.9.0 windows)](#windows090)
- [src/features (0.10.0 panels)](#panels0100)
- [src/ui/dock.ts and the 0.11.0 features](#dock0110)
- [src/features/document: websites](#web0120)
- [src/features/clipboard.ts](#clip0180)
- [src/ui/command-form.ts](#cmdform0190)

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

### dom.ts, themes.ts, icons.ts, menu.ts, commands.ts, toast.ts, dialogs.ts, dropdown.ts, panel-group.ts · provenance
- refers-to: frontend/src/ui :: the files named
- kind: rationale
- logged: v0.1.0 · 2026-10-07

Copied verbatim from formvis 1.65.0 (Apache-2.0, the same owner; dom.ts, themes.ts and part of icons.ts are the NodeMode stylized design system, MIT). Their in-file comments are kept as they are upstream so a later family update can be diffed and taken whole; formdes's own files carry none. formdes adds icons by appending to `ICONS`, never by editing the family's glyphs. Local changes to take care of in a family update (0.11.0): `commands.ts` gains `released`, `menu.ts` refreshes each row's keys when a menu opens, `panel-group.ts` gains the `tabIds` and `minimised` getters. formvis's `dock.ts` was not taken: formdes's `ui/dock.ts` is its own, written for groups that features register.

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

---

## src/features (0.9.0 windows) {#windows090}

### find.ts · FindWindow
- refers-to: frontend/src/features/find.ts :: FindWindow
- kind: explainer
- logged: v0.9.0 · 2026-10-08

A floating window on `--z-companion`, not a Dialog: Dialog adds a scrim and counts as open in `anyDialogOpen`, which stops canvas keys, and Find/Change has to stay open while the person works on the page. All matching runs in the core (`findChange`); the window only builds the parameters. For the To End of Story and Selection scopes it snapshots the selection as `within` the first time a search runs and keeps it until a setting changes, because each Find Next moves the selection onto the match. Results are keyed by story, cell or note and start, patched in place; they clear on a tab change or a switch of document (by uid, since the `docs` event also fires on every edit). Find Next asks the viewport to `reveal` the match's canvas rect, which pans only when the match is off screen. Special-character menus are native selects shown through `enhanceSelect`, reset to `@` after each insert; codes with a gap (`()`, `(?<=)`, `[]`) leave the caret inside.

### find.ts · source text
- refers-to: frontend/src/features/find.ts :: visible, LINE_BREAK, MARKERS
- kind: rationale
- logged: v0.9.0 · 2026-10-08

The line separator and the private-use marker range are built with `String.fromCharCode`: written as escapes in a regex literal, an editing tool once turned them into raw characters, and a raw U+2028 ends a JavaScript regex literal.

### spelling.ts · SpellWindow
- refers-to: frontend/src/features/spelling.ts :: SpellWindow
- kind: explainer
- logged: v0.9.0 · 2026-10-08

Each step re-runs `spelling.check` without suggestions (cheap) and asks `spellSuggest` for the current word only. A cursor of story and byte offset marks where the check has got to; offsets are UTF-8 bytes, as the core counts them, hence `bytes()` after a change. The check goes round the document once from the cursor. Ignore All lasts until the window is reloaded; Add writes to the document's dictionary, so it travels with the file. Change All goes through `findChange` (whole word, case sensitive) so it is one undo step.

### viewport.ts · spelling marks and reveal
- refers-to: frontend/src/editor/viewport.ts :: fetchOverlays, drawOverlays, reveal
- kind: explainer
- logged: v0.9.0 · 2026-10-08

With `opts.spelling` on, each overlay fetch also asks the core for `spellMarks` in the visible rect, and the marks are drawn as a zigzag in `--danger` on the highlight canvas, under the frame edges, hidden in Preview. `reveal(rect)` does nothing when the rect is inside the view with a margin, fits it when it is bigger than the view, and otherwise centres it at the current zoom.

### document/preflight.ts · profiles
- refers-to: frontend/src/features/document/preflight.ts :: PreflightPanel
- kind: explainer
- logged: v0.9.0 · 2026-10-08

The panel calls the core's `preflight` with the chosen profile's checks rather than `preflight.run`. The working profile is a per-browser choice (localStorage), so two people sharing a document can check it for different destinations. Define edits a copy when the profile is built in.

## src/features (0.10.0 panels) {#panels0100}

### review.ts · LivePanel exported
- refers-to: frontend/src/features/review.ts :: LivePanel
- kind: rationale
- logged: v0.10.0 · 2026-10-08

The Review panels' base (load on show, reload on document change and selection, `run` that executes and reloads, errors as toasts) fits the Interactive and XML panels exactly, so it is exported and they extend it rather than carrying a copy each.

### interactive.ts · three panels
- refers-to: frontend/src/features/interactive.ts :: StatesPanel, InteractivePanel, TransitionsPanel, installInteractive
- kind: explainer
- logged: v0.10.0 · 2026-10-08

All three sit in one side-column group, `interactive`, which starts minimised. States reads `states.list` for the selected object and renames in place with an inline field (Enter keeps, Escape restores). The Buttons and Forms panel works on one selected object: one round of `button.list`, `form.list` and `media.get` decides its type and which rows show (`.fin-b` for buttons, `.fin-x` for fields), and changing the type clears the old button or field (`button.clear`, `form.clear`) before `button.set` or `form.set`, so an object is never both. The media block shows only when `media.get` answers for the object, so the panel has no media kinds of its own. Transitions lists only spreads with a transition, and the speed is in seconds as InDesign shows it.

### xml.ts · Tags and Structure
- refers-to: frontend/src/features/xml.ts :: TagsPanel, StructurePanel
- kind: explainer
- logged: v0.10.0 · 2026-10-08

Clicking a tag both picks it (for Delete) and applies it when something is selected: text when the selection has a range, otherwise the selected frames, which is how InDesign's Tags panel behaves. Tag names are checked against XML's name rule before the core is asked. The style mapping list is built from the document's paragraph styles and patched in place; a select's options are replaced only when the tag list changes (`dataset.key`). Export builds the file in the window and downloads it, since the core already returns the text; Validate is enabled only while a DTD is loaded, and its problems are cleared when the DTD changes.

### library.ts · panel and drop
- refers-to: frontend/src/features/library.ts :: LibraryPanel
- kind: explainer
- logged: v0.10.0 · 2026-10-08

The panel is plain REST over `/api/libraries`; the chosen library is remembered per browser. A thumbnail card's key includes the thumbnail's length, so a rename patches the card and only a new picture replaces its image. Cards drag with their own MIME type, so the stage's capture-phase drop handler takes only library drags and leaves file drops to the workspace; the drop point goes through `viewport.toCanvas` and the engine places the snippet's top left there. Thumbnails sit on `--fd-paper`, because a snippet is drawn on transparency and dark type would vanish on the panel's surface.

### shell.ts · startMin
- refers-to: frontend/src/ui/shell.ts :: PanelGroup.startMin
- kind: rationale
- logged: v0.10.0 · 2026-10-08

`interactive`, `xml` and `library` join the groups that start minimised, so a first run shows the panels most layouts use and the specialist ones wait behind a tab click.

## src/ui/dock.ts and the 0.11.0 features {#dock0110}

### dock.ts · Dock
- refers-to: frontend/src/ui/dock.ts :: Dock
- kind: rationale
- logged: v0.11.0 · 2026-10-08

The columns hold `PanelGroup` sections that features register, so the dock works on group ids rather than formvis's fixed list: the starting order is read from the DOM after `mountPanels`, a saved order is merged over it (a group newer than the saved state takes its default place), and groups made by dropping a tab between groups (`g` plus a time code) are recreated empty before tab homes are restored, then dropped once empty. Sections move only when the order differs, by appending the same nodes, never rebuilt; splitters are kept in a map by the group above them and reused, so a drag keeps its pointer capture. A group is dragged by its header except its buttons and, when it has several tabs, its tabs (those move a tab instead); the click that ends a drag is swallowed. Width and heights are written during the drag and saved on release; a double-click returns either to its default. A minimised group ignores its height (CSS) and its splitter does nothing.

### shell.ts · layouts, areas, keys
- refers-to: frontend/src/ui/shell.ts :: layout, applyLayout, menuAreas, onKeyCommand, groupMenuFor, menuItems
- kind: rationale
- logged: v0.11.0 · 2026-10-08

`layout()` is everything a workspace needs: the dock's state, each group's tab and minimised flag, and which group each tab lives in. `applyLayout` creates any custom groups first, sends every tab home (the layout's home or the group its feature registered it in), removes custom groups the layout does not name, then sets tabs and minimised flags, so a built-in workspace needs to say only what differs. `menuAreas()` walks the built menus, which is how Keyboard Shortcuts knows each command's product area without a second list. A key that a command had by default but no command has now is swallowed, or the core would still act on its own copy of the shortcut. A core command whose cached state says disabled refreshes the states once and then runs: the states are fetched when a menu opens, so before 0.11.0 a shortcut after a selection change did nothing until a menu had been opened. Right-click on a group header opens Close Group and Minimise Group rather than a ••• button, which would have taken tab room in every header. Menu additions can go `top`, above the shell's own items (Window > Workspace).

### workspaces.ts · built-ins and your own
- refers-to: frontend/src/features/workspaces.ts :: BUILTIN, installWorkspaces
- kind: explainer
- logged: v0.11.0 · 2026-10-08

Built-in workspaces are written as columns, open groups, hidden groups and tabs, and turned into a layout when chosen; yours are whole layouts from `/api/prefs/workspaces`. Choosing one applies it; the arrangement you then make is kept as the live one, and Reset applies the chosen workspace again. The switcher is a select inserted into the title bar at install, before the window is revealed, and enhanced like every select; it and the Window menu's submenu are refreshed together, the select's options only when the list of names changes.

### shortcuts.ts · sets and tools
- refers-to: frontend/src/features/shortcuts.ts :: installShortcuts, ShortcutsDialog
- kind: rationale
- logged: v0.11.0 · 2026-10-08

A set is the difference from the defaults (`commands.overrides()`), so a set made today still gives new commands their default keys. The active set's keys are cached on this device and applied when the core's commands arrive, before the engine answers, then compared with the saved copy. The default set is read-only as in InDesign: a first change asks for a name and makes a set. Tools become commands (`tool.ID`) carrying the core's tool keys, so they are listed and rebindable; single-key commands are not matched while text is being typed, as before. New shortcuts are captured in a read-only field that takes every key but Tab (focus) and Escape (clears), and the field names the key's current owner before Assign moves it. Show Set writes the set as text by product area.

### proof.ts · Proof Setup and Proof Colors
- refers-to: frontend/src/features/proof.ts :: installProof
- kind: explainer
- logged: v0.11.0 · 2026-10-08

The setup (target, intent, paper) is remembered on this device; Proof Colors puts a copy on the viewport's options and redraws every tile, and turns itself off when no document is open. The custom dialog lists Working CMYK, every CMYK profile the core knows (`color.settings`), and the RGB and vision targets; Load Profile uploads the file and posts its path to `/api/prefs/profiles`, which keeps it. The status bar chip is empty (and takes no room) while proofing is off.

### dictionaries.ts, spelling.ts · languages
- refers-to: frontend/src/features/dictionaries.ts :: DictionariesDialog; frontend/src/features/spelling.ts :: SpellWindow.list, SpellWindow.next
- kind: explainer
- logged: v0.11.0 · 2026-10-08

Check Spelling asks the core's `spellCheck` (not the `spelling.check` command) so each word comes with its language; the window shows the language and asks `spellSuggest` with it. The dialog shows each language's state from the engine's listing: built in, installed, using another region's dictionary, or not installed; Get, Remove and the licence link follow from the same listing, and every action redraws from the listing it returns. Add Hunspell Files installs a picked pair for the language picked in the list.

## src/features/document: websites {#web0120}

### web-options.ts, export.ts · the Website format
- refers-to: frontend/src/features/document/web-options.ts :: WebOptions; frontend/src/features/document/export.ts :: ExportDialog.ext, syncName, options, run
- kind: rationale
- logged: v0.12.0 · 2026-10-08

One `WebOptions` block serves both Export and Publish Online, so the two never disagree about what a site option means. Empty fields are left out of the options (the engine reads title, description and language from the document); Edition is a three-way choice because its automatic default depends on the document. A website downloads as a .zip but goes into the Exports folder as a folder, so switching the destination renames the file field. A bad address stops the run before the engine is asked. Both dialogs give focus back to their main button after a run: the button is disabled while working, the browser moves focus to the page, and Escape then never reached the dialog.

### publish.ts · PublishDialog
- refers-to: frontend/src/features/document/publish.ts :: PublishDialog.refresh, fill, act, publish
- kind: rationale
- logged: v0.12.0 · 2026-10-08

The engine says which site belongs to the open document (`active`), because an unsaved document has no path the window could match. Rows are keyed by slug and patched in place, so a republish changes only the size and time text (checked under 4x CPU and a slow network: no rows added or removed). Unpublish needs a second click within three seconds rather than a confirm dialog, which would block the window. Links open through the engine (`/api/system/open`), which works the same in the browser and the desktop shell, and fall back to `window.open`.

## src/features/clipboard.ts {#clip0180}

### clipboard.ts · installClipboard
- refers-to: frontend/src/features/clipboard.ts :: writeSystem, readSystem, viewTarget, installClipboard
- kind: rationale
- logged: v0.18.0 · 2026-10-10

The layout core keeps the real clipboard (objects with their stories and pictures, formatted text); the window only mirrors its plain text onto the system clipboard after Copy and Cut, and hands the system clipboard's text to Paste so text copied in another app pastes in. Reading the system clipboard can prompt or hang in some browsers, so the read gives up after 600 ms and the paste goes ahead with formdes's own clipboard. With no text cursor, Paste sends the spread of the page in view and the view's centre in that spread's coordinates (the inverse of the slot transform from the layout), so pasted objects land where the person is looking. Mod+V reaches the command layer before the browser's paste event, which is why the old paste listener in shell.ts no longer sees canvas pastes.

## src/ui/command-form.ts {#cmdform0190}

### command-form.ts · dialogs from command signatures
- refers-to: frontend/src/ui/command-form.ts :: parseSig, kindOf, needsForm, openCommandForm, DYNAMIC
- kind: rationale
- logged: v0.19.0 · 2026-10-10

Every engine command carries a signature string (`{name?: a|b, n: pt, ...}`), so one form covers all of them instead of a dialog per command. `parseSig` splits the top level by commas and alternatives by `|` (a field after `|` starts a new alternative group, a bare word continues a choice list), and `kindOf` picks the control: a choice list, a JSON box for a value described as a list or object, then check boxes and numbers by name or unit word, file pickers for paths, and two special fields (TOC entries with style and level, OpenType feature check boxes). Choices that depend on the document (styles, swatches, anchors, variables, fonts, inks, lists, printers, profiles, hyperlinks, bookmarks, data fields) load from query commands through `DYNAMIC`, keyed `command.field`. A collapsed Parameters as JSON box is merged over the fields, so anything the form does not model can still be sent. A result carrying base64 is downloaded and a result message shows as a toast.

### shell.ts · menu shape
- refers-to: frontend/src/ui/shell.ts :: menuShape, prefixCache
- kind: rationale
- logged: v0.19.0 · 2026-10-10

Engine menu paths end with the item's own text (`["Object", "Pathfinder"]`), so the last element is the leaf label, unless that whole path is a prefix of another command's path, in which case it is a submenu and the command's own label is the leaf inside it. Without this, single commands showed as one-item submenus. Open menus refresh their enabled states when the core answers, because enablement depends on the selection the core reports.

### text-options.ts · Paragraph and Character Options
- refers-to: frontend/src/features/text-options.ts :: installTextOptions
- kind: rationale
- logged: v0.19.0 · 2026-10-10

The two dialogs are built from `type.attrsCatalog` (each model attribute's name, level, kind, description and the current value at the cursor) and send only the changed values through `type.attrs`, so the panels and the dialogs never disagree and a new attribute in the model appears here without window code. Attributes are grouped in collapsible sections, and list- or object-valued ones (tabs, nested and GREP styles, rules, shading) are JSON boxes.

## src/ui/dock.ts: the left column and the minirail {#cols0200}

### dock.ts · three columns, collapse and the minirail
- refers-to: frontend/src/ui/dock.ts :: DockState, merge, apply, setCollapsed, renderStrips, stripButton, placeCollapse
- kind: rationale
- logged: v0.20.0 · 2026-10-12

The family's interface standard (FormWeb 0.3.4, formvis 1.66.1) puts one minirail on the left and makes every panel column collapsible. The dock now has three columns, `left` (between the tool rail and the canvas), `side` and `dock`, and `collapsed` lists the folded ones. The left column folds into the tool rail's lower part (`.fd-minirail`), so the window keeps one rail; the right columns fold into the strips formvis uses (`.cv-props-strip`, `.cv-dock-strip`). A strip or minirail icon moves the group's own section into one flyout in `#overlay-root` and leaves a comment where it was, and closing puts it back, so nothing is rebuilt and a panel's scroll, inputs and selection survive. The left column's width is one variable (`--fd-left-w`, 252 px open, 0 folded or empty) and the right strips set `--cv-props-w` and `--cv-layers-w` to 46 px through classes on the app, so a collapse writes one grid track and moves no node. A saved dock state from before 0.20.0 has no `left` list; `merge` then moves the groups whose panels register `column: 'left'` (Pages and Make by default) out of the right columns once, and keeps the person's arrangement after that, including panels they dragged into or out of the left column. Workspaces carry `left` and `collapsed`; built-ins put Pages and Make on the left unless they hide them.

### columns.css · the grid with a left column
- refers-to: frontend/src/styles/columns.css
- kind: rationale
- logged: v0.20.0 · 2026-10-12

Loaded after formdes.css and before light-mode.css and scrollbars.css. It redefines `.cv-main`'s tracks for six columns, keeps the options bar across rail, left column and canvas, and repeats the Hide Panels rule for six tracks; the five-track rules in shell.css stay for formvis parity and are overridden here.
