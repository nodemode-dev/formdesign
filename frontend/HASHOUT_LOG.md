# HASHOUT_LOG · frontend · FormDesign

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
│   ├── favicon.svg, favicon.ico, apple-touch-icon.png   the FormDesign mark
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
    │   └── formdesign.css        FormDesign's own rules, tokens only
    └── ui/
        ├── dom.ts, themes.ts     zero-flash helpers and the 14 profiles (vendored)
        ├── icons.ts              inline currentColor SVG set (vendored, grows with FormDesign's own)
        ├── menu.ts, commands.ts, toast.ts, dialogs.ts, dropdown.ts, panel-group.ts   family controls (vendored; panel-group adds startMin and border-aware tab fitting)
        ├── shell.ts, workspace.ts   the Shell implements the Workspace contract features install into
        ├── dock.ts               the two panel columns: order, hidden groups, heights, width, groups made from dropped tabs
        ├── tool-icons.ts         the tool rail glyphs
        ├── brand.ts              the FormDesign mark
        ├── storage-migrate.ts    moves browser storage keys saved under the previous name to the `formdesign.` prefix once at start
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

Verbatim from formvis 1.65.0 (stylized-nodemode theme-boot.js, appearance generation 3, default profile `studio`, accent `#f97316`): the persisted profile and accent land on the root before the body paints, which is the whole mechanism behind no theme flash on reload. FormDesign shares the storage key `nodemode_ui_settings` with the family on purpose, so a theme chosen in formvis carries over. `formdesign.css` loads after the family sheets and before `scrollbars.css`, which stays last.

---

## src/app.ts {#app}

### app.ts · SHELL
- refers-to: frontend/src/app.ts :: SHELL
- kind: explainer
- logged: v0.1.0 · 2026-10-07

The shell is formvis's grid (title bar, options row, rail, stage column with document tabs, a properties column, a resizer and a dock column, status bar) mounted once. In FormDesign the options row is the Control panel, the rail holds the core's tool catalog, and the stage carries three canvases: `fd-tiles` (pixels from the core), `fd-hilite` (text highlight, blended as difference so selected type inverts) and `fd-overlay` (selection, guides, tool feedback).

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

Copied verbatim from formvis 1.65.0 (Apache-2.0, the same owner; dom.ts, themes.ts and part of icons.ts are the NodeMode stylized design system, MIT). Their in-file comments are kept as they are upstream so a later family update can be diffed and taken whole; FormDesign's own files carry none. FormDesign adds icons by appending to `ICONS`, never by editing the family's glyphs. Local changes to take care of in a family update (0.11.0): `commands.ts` gains `released`, `menu.ts` refreshes each row's keys when a menu opens, `panel-group.ts` gains the `tabIds` and `minimised` getters. formvis's `dock.ts` was not taken: FormDesign's `ui/dock.ts` is its own, written for groups that features register.

---

## src/ui/brand.ts {#brand}

### brand.ts · MARK
- refers-to: frontend/src/ui/brand.ts :: MARK
- kind: explainer
- logged: v0.1.0 · 2026-10-07

The FormDesign mark in the family's construction (formvis's tile, rim and bloom, and its blue to violet to pink to amber paint gradient): an open spread, an image block and a headline bar in the paint gradient, text lines on paper. Ids are namespaced per copy so two marks on screen never share gradients. `assets/formdesign_icon.png`, the favicons and the splash are renders of this same SVG.

---

## src/ui/home.ts {#home}

### home.ts · HomeScreen
- refers-to: frontend/src/ui/home.ts :: HomeScreen
- kind: explainer
- logged: v0.1.0 · 2026-10-07

formvis's start screen recipe (the first action white as the call to action, the rest dark, shortcut key caps, a drop hint, recent documents below). FormDesign leads with Make a layout, then New document and Open. It shows with no document open, or when forced from the brand button; Esc leaves it while a document is open.

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

Modes are Off, Separations and Ink Limit. The panel only sets `viewport.opts.plate` and `viewport.opts.inkLimit` and invalidates the tile cache; formdesign-core renders the plate (0 to 3) as greys, or sums the four plates and paints red where the total passes the limit. The limit is clamped to 100 to 400. Overprint Preview is a separate tile option so it can be on in any mode; the View menu's checked state reads the same option.

### review.ts · ChangesPanel, NotesPanel, ConditionsPanel
- refers-to: frontend/src/features/review.ts
- kind: explainer
- logged: v0.8.0 · 2026-10-08

Three panels in the side column's Review group, each a keyed `patchList` over the core's lists (`changes.list`, `note.list`, `condition.list`) reloaded on document change. Track changes reads its checkbox from `editor.doc.trackChanges`, which formdesign-core adds to each entry of the document list, because `inspect` settings do not carry it. Accept and reject address a change by story and start offset.

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

All three sit in one side-column group, `interactive`, which starts minimised. States reads `states.list` for the selected object and renames in place with an inline field (Enter keeps, Escape restores). The Buttons and Forms panel works on one selected object: one round of `button.list`, `form.list` and `media.get` decides its type and which rows show (`.fin-b` for buttons, `.fin-x` for fields), and changing the type clears the old button or field (`button.clear`, `form.clear`) before `button.set` or `form.set`, so an object is never both. The media block shows only when `media.get` answers for the object, so the panel has no media kinds of its own. Transitions lists only spreads with a transition, and the speed is in seconds.

### xml.ts · Tags and Structure
- refers-to: frontend/src/features/xml.ts :: TagsPanel, StructurePanel
- kind: explainer
- logged: v0.10.0 · 2026-10-08

Clicking a tag both picks it (for Delete) and applies it when something is selected: text when the selection has a range, otherwise the selected frames. Tag names are checked against XML's name rule before the core is asked. The style mapping list is built from the document's paragraph styles and patched in place; a select's options are replaced only when the tag list changes (`dataset.key`). Export builds the file in the window and downloads it, since the core already returns the text; Validate is enabled only while a DTD is loaded, and its problems are cleared when the DTD changes.

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

A set is the difference from the defaults (`commands.overrides()`), so a set made today still gives new commands their default keys. The active set's keys are cached on this device and applied when the core's commands arrive, before the engine answers, then compared with the saved copy. The default set is read-only: a first change asks for a name and makes a set. Tools become commands (`tool.ID`) carrying the core's tool keys, so they are listed and rebindable; single-key commands are not matched while text is being typed, as before. New shortcuts are captured in a read-only field that takes every key but Tab (focus) and Escape (clears), and the field names the key's current owner before Assign moves it. Show Set writes the set as text by product area.

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

The layout core keeps the real clipboard (objects with their stories and pictures, formatted text); the window only mirrors its plain text onto the system clipboard after Copy and Cut, and hands the system clipboard's text to Paste so text copied in another app pastes in. Reading the system clipboard can prompt or hang in some browsers, so the read gives up after 600 ms and the paste goes ahead with FormDesign's own clipboard. With no text cursor, Paste sends the spread of the page in view and the view's centre in that spread's coordinates (the inverse of the slot transform from the layout), so pasted objects land where the person is looking. Mod+V reaches the command layer before the browser's paste event, which is why the old paste listener in shell.ts no longer sees canvas pastes.

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

Loaded after formdesign.css and before light-mode.css and scrollbars.css. It redefines `.cv-main`'s tracks for six columns, keeps the options bar across rail, left column and canvas, and repeats the Hide Panels rule for six tracks; the five-track rules in shell.css stay for formvis parity and are overridden here.

### viewport.ts · resize draws in the same frame
- refers-to: frontend/src/editor/viewport.ts :: resize
- kind: rationale
- logged: v0.20.1 · 2026-10-12

Setting a canvas's width clears it, and a ResizeObserver callback runs after the frame's animation callbacks, so a redraw queued from it lands a frame late and the cleared canvas paints once. resize() therefore draws tiles and overlays synchronously from the tiles it already holds, then queues the normal pass. With a document open it shifts the view by half the size change in each direction so the point at the stage's centre stays put when a column folds or the window resizes.

### dock.ts · the left column's resizer
- refers-to: frontend/src/ui/dock.ts :: wireLeftResizer, merge (leftWidth)
- kind: rationale
- logged: v0.20.1 · 2026-10-12

The handle lives inside the left column (absolute, right edge) rather than as a grid track, so the grid keeps six tracks and folding still writes one variable. `leftWidth` is optional in DockState so layouts saved by 0.20.0 and the built-in workspaces stay valid; null means the 252 px default.

## src/features/terminal.ts {#terminal0210}

### terminal.ts · the Terminal panel
- refers-to: frontend/src/features/terminal.ts :: load, theme, installTerminal; frontend/src/styles/dock.css (terminal.css until 0.78.0)
- kind: rationale
- logged: v0.21.0 · 2026-10-12 · superseded in part at v0.78.0: the terminal moved from a panel group into the bottom dock, see the 0.78.0 entry

The owner chose a dockable panel over a drawer, so the terminal is an ordinary panel group (`terminal`, home column `left`, minimised at start) and gets folding, flyouts, workspaces and drag for free. xterm is imported on first show only, so the window's first paint does not pay for it. One `.ft-screen` per session stays mounted; a tab switch flips `hidden`, and the flyout moves the panel's own node, so a running CLI never loses its screen. Resize goes through one rAF-coalesced fit, and xterm's onResize sends the new size to the PTY. Colours come from the theme tokens at creation and on `nm:ui-settings-changed`, assigned to `term.options.theme`, which xterm applies without a remount. xterm 6 draws its own scrollbar elements inside `.xterm-scrollable-element`, which the global `::-webkit-scrollbar` rule cannot reach, so terminal.css hides them by class and the slider colours are transparent as well. Exit is detected from the engine's `[process exited with code N]` line, which strikes the tab through.

## src/features/connect.ts {#connect0220}

### connect.ts · the Agent Connect panel
- refers-to: frontend/src/features/connect.ts :: ConnectPanel, installConnect; frontend/src/engine/client.ts :: receive; frontend/src/ui/shell.ts :: publishActions, agentView
- kind: rationale
- logged: v0.22.0 · 2026-10-12

The engine announces agent activity as `{type: "agents", what}` events on the window's socket rather than sending the lists, so the panel refetches one snapshot (`/api/agents`, coalesced while a fetch is running) and patches its four keyed lists; nothing in the panel is rebuilt, and a select the person is using is not overwritten. An approval request opens the panel and raises a toast, because the agent is blocked until the person answers. Follow the agent listens to `changed` events from source `mcp` and reveals the union of their damage rectangles, skipping structural changes (no damage, or a damage box larger than the pasteboard). `view` events from the `show` tool go to the shell, which reuses showPanel, setTool, showPage and the zoom commands, so an agent can move the person's view only in ways a person could. The shell posts its own commands (ids, labels, keys, menu) to `/api/actions/window` once, after the menus exist, so the engine's catalogue includes window-only actions.

## src/features: row editors, colour tools, autocorrect (0.23.0) {#parity0230}

### attr-editors.ts · no JSON in dialogs
- refers-to: frontend/src/features/attr-editors.ts :: structuredEditor, listEditor, objectEditor, triple, gradientStops; frontend/src/features/text-options.ts :: open
- kind: rationale
- logged: v0.23.0 · 2026-10-12

The parity trace kept GREP styles, nested styles, rules, shading and gradients partial because they were typed as JSON. Each list or object attribute the catalogue reports now has an editor whose reader returns exactly the JSON shape the model parses (`grep: [{style, pattern}]`, `nested: [{style, through, count, inclusive}]`, `tabs: [{position, align, leader, alignOn}]`, rules `{on, weight, color, tint, width, offset, left, right}`, shading `{on, color, tint, top, bottom, left, right}`, spacing triples), so Paragraph Options still sends only changed values and the engine is unchanged. Percent fields scale on read and write (tints are 0 to 1 in the model). The nested delimiter list is the one the composer matches (`para.rs nested_ranges`), so nothing offered is ignored at composition; nested line styles are not offered because the composer has none yet. A structured field is a div, not a label, so clicking inside it does not activate its first control.

### colour-tools.ts and autocorrect.ts
- refers-to: frontend/src/features/colour-tools.ts :: gradientDialog, colourTheme; frontend/src/features/autocorrect.ts :: installAutocorrect
- kind: rationale
- logged: v0.23.0 · 2026-10-12

New Gradient Swatch is overridden with a stop list. Colour Theme from Image uploads the picture, asks the engine service for its palette (the same median-cut palette intake uses) and adds the ticked colours plus their colour group through `script.run`, one undo step. Autocorrect's list is the person's, kept in the browser and pushed to the engine on each `hello`, because the engine's settings are per session; the core's `edit.autocorrect` menu entry opens the dialog and a separate Enable Autocorrect entry toggles it.

## src/features: pdf-comments, review-share, word-import (parity pass 4c) {#review0250}

### pdf-comments.ts and review-share.ts
- refers-to: frontend/src/features/pdf-comments.ts :: PdfCommentsDialog; frontend/src/features/review-share.ts :: ShareDialog, ReviewPanel
- kind: rationale
- logged: v0.25.0 · 2026-10-12

Each dialog is built once and its lists are patched by key (imported notes by story and id, comments by id, replies by position), so a second import or a poll never rebuilds a row, and a reply being typed survives the three second refresh. The Review panel polls `/api/review` only while it is visible; a reviewer's comment also changes the document, so the Notes panel updates through its normal change event. The panel shows who and where; Go to selects the comment's Note through `note.list` (byte offsets, as `text.select` takes).

### word-import.ts
- refers-to: frontend/src/features/word-import.ts :: WordImportDialog, syncOptions; frontend/src/ui/shell.ts :: onPlaceFile
- kind: rationale
- logged: v0.25.0 · 2026-10-12

File > Place with one .docx offers it to `onPlaceFile` handlers before the plain Place; cancelling the dialog counts as handled. The two style tables are keyed rows whose select options are patched, not rebuilt; a Word style with the same name as a document style defaults to that style, otherwise to Use Word style, and a remembered pick wins. After a Place into the cursor the window asks the engine to finish the pending character styles and footnotes on each change for up to 30 minutes.

## src/features/properties: variable font axes (0.26.0) {#axes0260}

### axes.ts
- refers-to: frontend/src/features/properties/axes.ts :: VariationAxes; frontend/src/features/properties/textpanels.ts :: CharacterPanel
- kind: rationale
- logged: v0.26.0 · 2026-10-12

The Character panel's axis sliders are built once per set of axes (a signature of tag, minimum and maximum), not per selection: moving between two fonts with the same axes only rewrites the slider values and readouts, and a font without axes hides the block. `type.fontAxes` answers are cached per family and style, and a slow answer for an earlier selection is dropped. A slider's readout follows the pointer; the change is sent on release or a key step as `fontVariations` (only axes that differ from the named style's coordinates, plus any already set), and Reset sends null.

### features/claude.ts, ui/markdown.ts · the Claude Code panel
- refers-to: frontend/src/features/claude.ts :: ClaudePanel; frontend/src/ui/markdown.ts :: MarkdownStream; frontend/src/features/terminal.ts :: claudeTerminal
- kind: rationale
- logged: v0.26.0 · 2026-10-16

The panel shares the Terminal's group in the left column (Claude Code first), as formlit's dock puts its Chat tab beside the terminals; the group no longer starts minimised. It never rebuilds its log: a user line and a reply container are appended per turn, streamed text goes through MarkdownStream (finished blocks are rendered once and inserted; only the unfinished tail paragraph's text changes), tool chips are appended and only their class changes on completion, and images keep fixed 160 by 120 boxes. Clearing the log happens only on New chat and adopt, which are a deliberate reset. Events arrive on the engine socket (`claude` type) rather than a per-request stream, so a reload, a second window or a turn started from another window all show the same conversation. The open document, the page in view and the selected item ids are appended to the prompt in parentheses, with a short note under the message. Shift+Mod+J opens a connected Claude Code terminal, Alt+Mod+J the panel; neither was bound in the engine or the window.

### ui/i18n.ts, ui/i18n/ · 26 interface languages
- refers-to: frontend/src/ui/i18n.ts :: t, tf, setLanguage, startLanguage, startPageTranslator; frontend/src/ui/i18n/langs.ts; frontend/src/ui/i18n/tables/*.json; frontend/src/ui/i18n/keys.json; setup/i18n_keys.py
- kind: rationale
- logged: v0.26.0 · 2026-10-16

The API and the page translator are formvis 1.72.8's (same owner): English is the source text and the key, `t()` sets a trailing "...", "…" or ":" aside, a regional variant (es-MX, fr-CA) falls back to its parent and then to English, and a MutationObserver translates text nodes and title, aria-label and placeholder attributes by exact match before the browser paints, keeping each node's English so switching back restores it. Text the person owns is skipped: inputs, editable text, the canvas, the terminal, Claude's and the Assistant's messages, code, and anything under `data-no-i18n`. The language is stored in localStorage `formdesign.language`; main.ts loads that table before the window is built, so there is no English first paint. Tables are one JSON file per language, keyed by English, loaded lazily through `import.meta.glob`. `setup/i18n_keys.py keys` collects the strings from the window's templates, attributes, label and title properties, toasts, options and the engine's command labels, menu paths and help; it rejects code-shaped matches. At 0.26.0 the 1,405 strings of 40 characters or fewer are translated in every language (menus, commands, panel and dialog labels, buttons); the 114 longer help sentences show in English until the next pass, which `./formdesign i18n check` lists. "Type" is the typography menu in every table, not "kind". Menu names built in the shell from its own list (File, Edit, Help and the rest) are translated when they are keys; Help was not collected yet. At 0.27.0 the extractor reads the shell's MENUS list and every one of the 1,526 strings is translated in all 23 full languages.

### ui/commands.ts, ui/shell.ts · the window's half of the command catalogue
- refers-to: frontend/src/ui/commands.ts :: Command, CommandParam, Commands.call, onChange, blocked, why; frontend/src/ui/shell.ts :: publishActions, sendCatalogue, runForAgent
- kind: rationale
- logged: v0.27.0 · 2026-10-16

The registry has formvis's fields and methods (group, description, when, params, mutates, risky; run with parameters, call that awaits, onChange, a blocked reason). The shell re-sends the catalogue 400 ms after any registration change (plugins, workspaces and shortcut sets add commands after start) and again when the engine connection reopens; the older `/actions/window` post is kept for `./formdesign actions`. Commands an agent runs arrive on the engine socket and go through `call`, so a disabled command answers with its reason instead of running.

### editor/viewport.ts · corner handles
- refers-to: frontend/src/editor/viewport.ts :: drawSelection (corners)
- kind: rationale
- logged: v0.27.0 · 2026-10-16

The engine's overlay lists a handle per corner anchor of each item under Direct Selection (`corners`: handle point, vertex, radius, picked). The window draws a ringed dot there, pushed out to at least 14 screen pixels from the vertex so it never sits on the anchor square at low zoom; the engine hit-tests the same pushed-out point, so what is drawn is what is grabbed. Canvas-drawn, no DOM.

### features/actions.ts · the Actions panel
- refers-to: frontend/src/features/actions.ts :: ActionsPanel; frontend/src/editor/editor.ts :: onExec; frontend/src/ui/shell.ts :: runCore; backend/app/routers/actionsets.py
- kind: rationale
- logged: v0.27.0 · 2026-10-16

A most-requested missing feature after table and footnote fixes (UserVoice "Actions Panel", 432 votes). Recording hooks the two places the window runs engine commands (`editor.exec` and the shell's `runCore`, which menus, the palette and the parameter dialogs use) and keeps undoable commands with their parameters, skipping selection, undo and redo, view and file-writing commands. Saved actions live in the engine's data folder (`actions.json`, `/api/actionsets`), so every window and agent sees the same list. Playing sends the steps as one batch, so an action is one undo step and fails as a whole. The list is patched in place by key. Window commands `actions.record`, `actions.stop` and `actions.play {name}` put it in the catalogue for agents. Recorded steps keep the object ids they acted on; an action whose steps name no ids acts on the selection when played.

### features/drawmode.ts · drawing mode and isolation chip
- refers-to: frontend/src/features/drawmode.ts :: installDrawMode; frontend/src/editor/editor.ts :: doSync (drawMode, isolated, 'mode'); frontend/src/editor/viewport.ts :: drawOverlays (isolated veil)
- kind: rationale
- logged: v0.54.0 · 2026-10-17

The drawing mode (Draw Behind, Draw Inside) and the isolated group live in the engine's tool state and reach the window with the `state` the editor already polls; switching either bumps the engine's selection revision, so no new event is needed. The chip is created once at install and sits directly after the flexible hint, so when its text appears only the hint gives up room and nothing else in the status bar moves (placed before the version label it pushed the renderer chip 92 px, caught by the throttled check); it is hidden while empty. Clicking it leaves isolation, or returns drawing to normal. The isolation veil is painted on the highlight canvas from the overlay's `isolated` rectangle (the group's bounds), so entering and leaving costs no DOM work and selection handles stay above it.

### features/artboards.ts · artboard outlines and PNG export
- refers-to: frontend/src/features/artboards.ts :: installArtboards, exportArtboards; frontend/src/editor/viewport.ts :: drawOverlays (artboards); backend/app/routers/core_api.py :: artboard_png
- kind: rationale
- logged: v0.59.0 · 2026-10-17

Artboards are document data in the core (`artboard.*` commands); the window only draws what the overlay lists and asks the engine for pixels. Outlines and names are stroked on the highlight canvas with the guides (hidden in preview), so adding or moving an artboard costs no DOM work. Export asks `artboard.list`, then fetches `GET /api/core/artboard.png` per artboard at 2x and clicks a detached download anchor; nothing is inserted into the page, so the throttled check saw no layout change. It answers for the active document, so a window with several Untitled documents open exports the one in front (a probe that ran against a fresh document without artboards got the toast and no downloads, which first looked like a lost click).

PDF export (v0.60.0) goes through the socket rather than a REST route: `artboard.exportPdf` without a path answers with base64, which the window decodes into a Blob and downloads as `<title> artboards.pdf`. One engine call builds every page, so the throttled check made a single round trip instead of one fetch per artboard.

### features/appearance.ts · the Appearance panel
- refers-to: frontend/src/features/appearance.ts :: AppearancePanel, rowsOf, effectName; frontend/src/ui/dom.ts :: patchList; native/formdesign-engine/src/cmd/layout.rs :: item_json (appearance, effects)
- kind: rationale
- logged: v0.69.0 · 2026-10-17

The panel reads the cached `editor.inspect()` (the document's items now carry `appearance` with swatch names and `effects` when an object has them) instead of a new per-item call, and works from a signature of the selected object's rows: when the stack has not changed, a selection or document event costs one string compare and no DOM work. Rows are keyed (`a<i>` extra paints, `stroke`, `fill`, `e<i>` effects) and patched in place; the eye icon is swapped only when its state flips. Hiding and showing go through `{index, on}` on `object.appearance` and `object.effect`, which keep the entry's settings, so the panel never rebuilds an entry from what it displays. Effect names come from a function of literal `t()` calls because the key extractor only sees literals; a lookup table of strings passed to `t()` left them untranslated. The panel has no Window command of its own: the shell lists every registered panel in the Window menu, and a second command made a duplicate entry. Writing it found a flaw in the shared `patchList`: stale rows were removed after ordering, so a removed first row left the cursor on it and every later row was moved in front; stale rows now go first.

### features/image-edit.ts · the Edit Image dialog
- refers-to: frontend/src/features/image-edit.ts :: EditImage, tone, adjust, installImageEdit; frontend/src/styles/image-edit.css; backend/app/routers/core_api.py :: asset_bytes
- kind: rationale
- logged: v0.70.0 · 2026-10-17

The signature-built dialog took crop and levels as text arrays, which made the command usable but not a parity answer, so `image.edit` is overridden with a dialog of its own. It fetches the asset's bytes once from `GET /api/core/asset`, downsamples them to 480 px on the long side, and keeps that ImageData; `tone` and `adjust` are a line-for-line port of pixedit's tone curve, so what the preview shows is what the engine writes. Slider input schedules one `requestAnimationFrame` that re-tones the small copy and repaints two fixed-size canvases (the picture with the crop shaded, and the result turned and flipped); readouts change text only. Under 4x CPU, 1.6 Mbps and 150 ms a crop drag, a turn and three slider moves made 0 structural DOM mutations. The crop is drawn on the unturned picture because the engine crops before it turns. A flip after a quarter turn is the other axis's flip before it, which is why the result canvas swaps the flips when turned. OK sends only the fields that differ from no change, and closes only when the command succeeds (`editor.exec` toasts the refusal).

### features/image-retouch.ts · the Retouch Image dialog
- refers-to: frontend/src/features/image-retouch.ts :: Retouch, installImageRetouch; frontend/src/styles/image-retouch.css
- kind: rationale
- logged: v0.71.0 · 2026-10-17

Two stacked fixed-size canvases: the picture (the engine's `image.retouchPreview` of every stroke so far, fetched quietly after each stroke, a later ticket winning over an earlier answer) and an overlay for the brush ring, the source crosshair and the stroke being painted, redrawn in one animation frame per pointer move. Alt-click sets the source; the first stroke after it fixes the offset, and later strokes keep it (aligned, the default), so the crosshair follows the brush. Painting on the source itself is refused in the note rather than sent, because the engine refuses a zero offset. The window check found `q(b, 'span')` picking the icon's own `<span class="icon">` and writing the label over the SVG, which clipped the tool buttons; the label span has its own class now. It also found attribute writes on every tool pick and every stroke (`aria-pressed`, the warn class); both are now written only when they change, so painting under throttling changed no DOM beyond Undo and Clear enabling once.

### features/pages-import.ts · Insert Pages from Document
- refers-to: frontend/src/features/pages-import.ts :: PagesImport, installPagesImport
- kind: rationale
- logged: v0.75.0 · 2026-10-17

A dedicated dialog over `layout.pages.import`, because the signature dialog would have asked for `from` as a bare number or path. The source list (the other open documents with their page counts, then "A file on disk...") and the position list (At the start, After page n) are rebuilt with `replaceChildren` when the dialog opens and never while it is open; choosing the file source only toggles the path row's `hidden` and the Move box's `disabled` through the guarded setters. Move is off for a file because files are never written. Under 4x CPU, 1.6 Mbps and 150 ms, filling the dialog made no DOM mutation and OK inserted the pages with the story threaded across them.

### features/image-adjust.ts · the Adjustments dialog
- refers-to: frontend/src/features/image-adjust.ts :: Adjustments, curveAt, installImageAdjust; frontend/src/styles/image-adjust.css
- kind: rationale
- logged: v0.76.0 · 2026-10-17

The preview is the engine's (`image.adjustPreview`), so it cannot drift from what the page and the PDF show. Moving a slider bumps a counter; one request is in flight at a time and, when it returns, another goes only if the counter moved, so a fast drag under 150 ms latency sends a handful of requests rather than one per input event and always ends on the latest settings. Every request carries the whole stack, which the engine lays over the stored settings, so the dialog never depends on what was stored. The curve editor is a canvas: click adds a point, drag moves it (the end points move only up and down, so the curve always spans 0 to 255), double-click removes it; it is drawn with `curveAt`, a port of the engine's monotone cubic. Slider labels are produced by arrow functions around literal `t()` calls, because the key extractor only sees literals and two of them (Hue, Lightness) were new. Under throttling the sliders, the Black and White box and a curve drag made no structural DOM change; OK, reopen (settings restored, Remove enabled) and Remove all reached the page.

### features/terminal.ts and features/claude.ts · the bottom dock for Terminal, Claude Code and Chat
- refers-to: frontend/src/features/terminal.ts :: TerminalDock, installTerminal, registerChat, openChat, documentContext; frontend/src/features/claude.ts :: ClaudePanel, AdoptDialog, installClaude; frontend/src/styles/dock.css; frontend/src/app.ts :: SHELL
- kind: rationale
- logged: v0.78.0 · 2026-10-19

The Terminal and Claude Code panels left the left panel group for one resizable dock under the canvas, inside the stage column, because a person working with Claude needs the page and the conversation in view together and a panel group hid one of them. The dock is static markup in `SHELL` (hidden at start) and `TerminalDock` only wires it, so opening it creates no nodes and the canvas keeps its top edge; only its height changes, which the viewport already follows. The height is kept in localStorage (`formdesign.dock.height`, every access in try/catch) and written when a drag ends. Ownership: `terminal.ts` owns the dock, the shell and Claude Code tabs (xterm, loaded on first use, its token-authenticated socket and the theme tokens) and the four commands; `claude.ts` owns the chat view (`ClaudePanel`), which the dock hosts as its Chat tab through `registerChat`, so neither file imports the other's internals and the chat's events and replay are unchanged. The Ask line belongs to the dock and sends to the chat on the Chat tab and otherwise opens or reuses the Claude Code tab (`reuse: true`, the prompt typed into the live session), always with the open document, page and selected item ids from `documentContext`. A hand-off from the chat starts a new Claude Code tab with `resume` and no reuse. Keys typed in the dock stop at the dock, because the editor's key router ignores text fields anyway and a terminal must receive every key; the one exception is Mod+J, handled by the dock itself so it hides the dock from inside xterm. Mod+J was Go to Page (`pages.goTo` takes it only through `free()`); the toggle is registered after Pages and wins, and Go to Page is left without a key rather than shown with a key it no longer answers to. The old terminal stylesheet is gone with the panel; `connect: agent` is the one icon change in the panel dock.

### home.ts, workspaces.ts, dock.ts · everyday first run
- refers-to: frontend/src/ui/home.ts :: TILES, HomeScreen; frontend/src/features/workspaces.ts :: BUILTIN, installWorkspaces; frontend/src/ui/dock.ts :: apply (is-dock-empty); frontend/src/styles/columns.css; frontend/src/styles/formdesign.css (title bar under 1180 and 1020 px); frontend/src/features/make/gallery.ts :: open
- kind: rationale
- logged: v0.78.0 · 2026-10-19

A first launch used to show the panels where each feature registered them, three crowded columns with cut-off tabs, because a workspace was only applied when chosen. Now, when localStorage has neither a chosen workspace nor a dock layout, Essentials is applied in a microtask after the panels mount, before the first paint, so nothing moves on screen. Essentials keeps Pages and Make on the left and Properties over Swatches on the side, and hides the rest (Window brings any back). A column a workspace leaves empty folds to zero width (`is-dock-empty`, not while a group is being dragged). Working with Claude runs `ui.claude` after its layout through the spec's `then`, built-in workspaces only. The Home screen's Start with tiles open the template gallery on a category (`ui.templates` with `{category}`; without one the gallery keeps its last filter), the Recent section stays mounted with a quiet line when empty so the first document does not shift the screen, and the title bar drops the zoom controls under 1180 px and undo and redo under 1020 px instead of overlapping the menu names.

### brand.ts, splash.html, src-tauri/icons · the app icon redrawn as a tilted magazine spread
- refers-to: frontend/src/ui/brand.ts :: MARK; frontend/public/favicon.svg; frontend/public/splash.html; src-tauri/icons/*; assets/formdesign_icon.png, formdesign_icon_macos.png, formdesign_folder_icon.png; assets/icon_src/
- kind: rationale
- logged: v0.79.0 · 2026-10-20

The flat spread kept its idea but now matches the redrawn family (FormDeck's fanned deck, formpdf's signed page, FormSurface's painted cube): the same tile, bloom, rim and drop shadow, the pages curving into the spine over a dark cover edge with a shadowed gutter, tilted five degrees, and one gradient hero image running across both pages under a headline bar, the spread layout a page-layout app is for. The macOS .icns uses the 824 px tile inside a 1024 canvas with a 100 px margin; .ico, the PNGs and the Square*Logo set stay full bleed as before. Sizes 64 px and below drop the body text for two heavy lines and a larger hero so the spread still reads at 16 px. MARK, favicon.svg and the splash share one filter-free drawing with a rounded-rect tile, keeping the {p} id prefix. assets/icon_src/ regenerates every file (python3 gen.py, node render.mjs, python3 build.py; python3 compact.py PREFIX prints the inline mark); the tile shadow is its own layer under an unfiltered tile, because a filter on the tile made Chromium band the gradient. compact.py strips the root's width and height once only: stripping every occurrence also took the size off the bloom rect, so the inline marks drew no bloom while the PNGs had one.

### ui/storage-migrate.ts, main.ts · the formdesign rename
- refers-to: frontend/src/ui/storage-migrate.ts :: migrateStorage; frontend/src/main.ts
- kind: rationale
- logged: v0.80.0 · 2026-10-20

Browser storage keys, DOM events, the library drag type, console prefixes, the plugin global and the stylesheet (`styles/formdesign.css`) all use `formdesign` now. A person's saved panel layout, units, workspace, shortcuts and the rest sit under keys with the short prefix, so `migrateStorage` runs first in main.ts, before the language and the features read anything: each such key is copied to its `formdesign.` name unless that is already set, then removed. On the desktop the storage itself lives in the webview's folder for the app identifier, which the shell moves to the new identifier before any window opens (root HASHOUT_LOG); this module then renames the keys inside it. The theme boot script in index.html reads the family key `nodemode_ui_settings`, which the rename does not touch.

### splash.html, dom.ts revealApp, src-tauri reveal.rs and traffic_lights.rs · the formvis splash and concealed reveal
- refers-to: frontend/public/splash.html; frontend/src/ui/dom.ts :: revealApp; frontend/src/app.ts :: startApp; frontend/index.html (has-overlay-titlebar, preload); frontend/src/styles/formdesign.css; src-tauri/src/reveal.rs; src-tauri/src/traffic_lights.rs; src-tauri/src/main.rs; src-tauri/tauri.conf.json
- kind: rationale
- logged: v0.81.0 · 2026-10-22

The desktop app now opens as formvis does: a 360 by 200 transparent splash card (rounded natively to 16 px on macOS, because the web view paints a square behind a CSS radius alone) with the app icon, Starting FormDesign and a bar that fills over 2.6 s; the main window is built concealed (hidden elsewhere, on screen at alpha 0 and ignoring the mouse on macOS, because a hidden web view stops animation frames) and is shown at max(2.6 s from launch, the page's app_ready), with a 20 s fallback. The splash no longer closes when the window is built; only an engine or build failure closes it early. revealApp keeps its two frames to body.app-ready and adds one more frame before invoking app_ready in the desktop shell; startApp waits for the font faces or 2.5 s before calling it, once. The page is a remote 127.0.0.1 origin, so app_ready and set_window_title have allow permissions of their own. On macOS the window uses an overlay title bar and traffic_lights.rs centres the lights in a 38 px row; index.html adds has-overlay-titlebar before first paint (macOS inside the desktop app only), which makes the title bar 38 px with 78 px reserved on the left, and the title bar is a drag region (start-dragging and toggle-maximize are allowed in the capability). The main window keeps its transparent flag for the native canvas and gets the near-black background colour as well; if the native renderer shows black where its hole should be, the colour is the cause. The splash hook is formdesignSplashDone, named in splash.html and reveal.rs. The in-page brand mark is the owner's icon file (/formdesign-logo.png, preloaded, width and height reserved) rather than a redrawn vector.

### ui/native-menu.ts, shell.ts buildMenus, src-tauri text_edit.rs · the macOS menu bar
- refers-to: frontend/src/ui/native-menu.ts :: NativeMenus, wantsNativeMenus, menusInWindow; frontend/src/ui/shell.ts :: buildMenus, uiCommands (ui.menusInWindow); frontend/src/styles/formdesign.css (has-native-menus); src-tauri/src/text_edit.rs; src-tauri/build.rs; src-tauri/capabilities/default.json
- kind: rationale
- logged: v0.82.0 · 2026-10-22

In the desktop app on macOS the in-window menus are also the system menu bar, built from the same `MenuDef` list and command registry as `MenuBar`, so labels, order, submenus and keys cannot differ; the list is read when each build starts, so it follows the core's command list, which arrives after the shell is built (`editor.on('commands')` rebuilds, overlapping builds fold into one). The `@tauri-apps/api/menu` import is dynamic so the web build never loads it. The app submenu is hand built (About, Preferences on `ui.prefs` with Mod+comma, Hide, Quit). The in-window bar hides only after a build succeeded (`has-native-menus`), so a failed build leaves the menus where they were; with it hidden the zoom group is pinned to its own grid column or it would slide into the bar's flexible column. Enabled, ticked and label states refresh 150 ms after a change; the core's enabled flags (which only `refreshEnabled` fetches) are re-read only for document, selection, tool and layout changes, not for every pointer and key event, to avoid a request per click. The six Edit commands (`ui.undo`, `ui.redo`, `edit.cut`, `edit.copy`, `edit.paste`, `edit.selectAll`) would take Cmd+Z, X, C, V and A from a focused text field, so while one has focus the item calls the shell's `text_edit`, which sends the standard AppKit action to the first responder; `text_edit` is allowed in build.rs and the capability as `app_ready` is, because the page is a remote origin. objc2 0.6 was already a macOS dependency and `text_edit.rs` uses only its runtime, so no feature was added. View > Menus in Window (`formdesign.menusInWindow`) applies at the next start. The comma, period and slash convert to accelerators so Preferences shows Cmd+comma; a combination the menu library rejects is rebuilt without its key.

### ui/caption-buttons.ts, shell.ts · View > Custom Title Bar on Windows and Linux
- refers-to: frontend/src/ui/caption-buttons.ts :: CaptionButtons, wantsCustomTitleBar; frontend/src/ui/shell.ts :: ui.customTitleBar; frontend/src/ui/icons.ts (capMin, capMax, capRestore); frontend/src/styles/formdesign.css (cv-caption); src-tauri/capabilities/default.json
- kind: rationale
- logged: v0.82.0 · 2026-10-22

Off by default (`formdesign.customTitleBar`); turning it on takes the system frame off with `setDecorations(false)` and shows Minimize, Maximize or Restore and Close at the right end of the title bar, built once and shown or hidden, with the Maximize icon swapped only when the window's state changes. Close goes through the window's close request. Unlike formvis, no pointer or double-click handlers are added: the title bar already carries `data-tauri-drag-region` from the overlay title bar work, and Tauri's own handler for it starts the drag and toggles maximize on a double click, so a second handler of ours would toggle twice and cancel itself. The group's own empty space (`.cv-menubar`, `.cv-zoom`) gets the attribute too, since Tauri acts only on the element that carries it and the buttons inside stay clickable. The glyphs are filled paths in the table's Material style rather than formvis's strokes. The page is a remote origin, so the window permissions (minimize, maximize, unmaximize, is-maximized, close, set-decorations) are listed in the capability beside start-dragging and toggle-maximize.

### ui/themes.ts, index.html · the family's 28 themes
- refers-to: frontend/src/ui/themes.ts :: THEMES; frontend/index.html :: theme-boot PROFILES, LIGHT
- kind: rationale
- logged: v0.82.0 · 2026-10-22

The theme choice is shared across the Form family (`nodemode_ui_settings`) and formvis offers 28 themes; FormDesign had 15, so a theme picked in formvis such as Pro Dark or Soft Light fell back to Studio. The 13 missing profiles are added with formvis's values to both tables, which must stay equal. Checked in headless Chromium with the store set to pro-dark, soft-light and nineties: the right `--bg-primary` at the first script turn and after start, `theme-light` on the light ones.

### features/document/prefs.ts, styles/dialogs.css, ui/icons.ts · the formvis Preferences dialog
- refers-to: frontend/src/features/document/prefs.ts :: PreferencesDialog; frontend/src/features/ai-settings.ts :: AiPage; frontend/src/styles/dialogs.css (cv-prefs block); frontend/src/ui/icons.ts (pfGeneral, pfInterface, pfUnits, pfPlugins, pfAssistant, pfFold)
- kind: rationale
- logged: v0.82.0 · 2026-10-22

Preferences is the formvis shell: a floating rail of section tabs (collapse state in `formdesign.prefsRailCollapsed`), one scrolling pane of foldable cards, a scroll spy, and an SVG wire and port from the active tab to the header, all built once and patched in place on open. The old four pages map to sections without renaming any stored key: Appearance to Interface, Units to Units & Rulers, Agents to Plugins, AI to AI; General is new and holds the read-only data folder and Reset All Preferences, which clears every `formdesign.` key and `nodemode_ui_settings` after a confirm. `open(page)` keeps its page ids and scrolls to the matching card. Workspace and Performance are not offered because the dialog has no rows for them yet; theme previews became the formvis swatch grid. New strings (section and group names, Reset text) are English only until the next translation pass.
