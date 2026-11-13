# FormDesign demo: A magazine feature

An offline demo of the FormDesign window that opens in a browser with no engine, no desktop shell, no network and no CDN. There are three pages; the other two are `demos/event` (An event poster) and `demos/report` (An annual review), and each page links to the others from the status bar.

## Open it

- Double-click `demo/index.html` (it works from `file://`), or
- serve the repository with any static server, for example `python3 -m http.server 8000`, and open `/demo/`.

Every script, style and font is local to the repository. The window's script, stylesheet, fonts and Home's demo covers are stored once, in `demo/assets/`; the two pages under `demos/` load them from there and carry only their own recording, so keep the folders together.

## What it shows

The page is the real FormDesign window, not a look-alike: `frontend/index.html` and the application's own entry (`frontend/src/main.ts`, then `app.ts` and every feature module, panel and menu) are bundled as they are. When the window is ready the page opens the demo exactly as Home > Demos does in the app, by clicking the "A magazine feature" card, while `html[data-demo]` keeps the window hidden; it is shown once, with the pages drawn.

"The Island Press" is a feature for Northlight, an invented island quarterly, made by Make from `backend/app/demos/magazine.md` on the `magazine-feature-spread` template: three pages in two spreads. Pages 2 and 3 face each other: a full-bleed picture with a caption on the left; on the right the kicker, the headline, the standfirst, the byline and the story set in threaded columns with a drop cap, subheads and a pull quote. The story continues on page 4, which is based on the A-Parent. The Pages panel shows the spreads and the parent, the Properties panel the document's size, margins, columns and bleed, and the frame edges, margins and columns are drawn over the pages.

## Recorded engine answers

FormDesign's layout core is native code (Rust) that the Python engine runs; it does all layout and drawing and is not built for the browser. So this page replays recorded engine answers. `demo/record.py` ran the real engine on its own port with an empty data folder, opened this demo from Home in the real window, opened every panel of every workspace, and kept what the window asked and what the engine answered: the document state and layout, the commands and tools, the panels' data, the page thumbnails and the socket greeting. It also had the engine draw every spread at 100% and 200%. Absolute paths and user names were removed. All of it is in `demo/assets/session.js` (about 0.8 MB). `src/shim/api.ts` answers the window's `/api` requests and its engine socket from that file, and draws each page tile the window asks for from the recorded spread pictures.

Live in the page (the window's own code, no engine needed):

- the window itself: menus, the tool rail, panels and their tabs, workspaces, Home, themes and languages;
- moving through the document: the Pages panel, the page and spread status, scrolling, zoom in and out, Fit Spread, and the zoom field;
- every panel and request the recording saw, answered as the engine answered then.

Replayed, so not live: what the panels show is the document as it was recorded. Zooming past 200% enlarges the 200% picture.

Needs the engine: anything that changes or reads beyond the recording, such as clicking or typing on the page, selecting, every command that edits the document, Undo, Save, Export, Print, Make, the assistant, Claude Code, the terminal, fonts and glyph previews, the overprint, separations and proofing views, and opening a second copy of a demo. These answer with the app's own error toast: "This needs the FormDesign engine. This page replays the answers the engine gave when the demo was recorded; editing, saving, export, the assistant and agents need the app." The status bar shows the recorded core version.

## Rebuild

The built files are committed. After changing the app or `demo/src`:

```sh
cd frontend
npm ci
npm run demo
```

`npm run demo` runs `vite build --config vite.demo.config.ts` three times, with `--mode magazine`, `event` and `report`, writing `demo/`, `demos/event/` and `demos/report/`. The three pages share `demo/src`; the mode only chooses the demo and the folder. `npx tsc --noEmit -p ../demo` type-checks the page code.

After a change to the engine, the layout core, the templates or the demo material, record again. Start the engine and the window with a data folder of their own, then run the recorder (it needs playwright, websockets, numpy and Pillow, and Chrome or Chromium):

```sh
export FORMDESIGN_DATA_DIR=/tmp/formdesign-demo
./formdesign boot
python3 demo/record.py magazine demo/assets/session.js
```

Restart the engine with an empty data folder before each of the three recordings (`event` to `demos/event/assets/session.js`, `report` to `demos/report/assets/session.js`).

## Licences

The bundle contains the app's own code (Apache-2.0), Inter and JetBrains Mono (OFL 1.1, via @fontsource) and xterm.js (MIT), all already in `frontend/package-lock.json` and `NOTICE.md`. See `CREDITS.md`.
