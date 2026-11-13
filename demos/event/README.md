# FormDesign demo: An event poster

An offline demo of the FormDesign window that opens in a browser with no engine, no desktop shell, no network and no CDN. It is one of three pages: `demo/` (A magazine feature, where the full notes are) and `demos/report` (An annual review); each links to the others from the status bar.

## Open it

Double-click `demos/event/index.html` (it works from `file://`), or serve the repository with any static server and open `/demos/event/`. The page loads the window's script, stylesheet, fonts and Home's demo covers from `demo/assets/`, where they are stored once, so keep `demo/` next to `demos/`.

## What it shows

The real FormDesign window, bundled from `frontend/` as it is, opens the "An event poster" demo the way Home > Demos does, by clicking its card while the window is hidden, and is shown once the pages are drawn.

"Quay Lights" is a poster for an invented harbour festival, made by Make from a short brief (headline, date, time, place, call to action, "bold and warm, navy and orange") on the `print-poster-event-type` template: an 18 by 24 inch page (1296 by 1728 pt) with the large two-line headline over an orange disc, the date and time, the three-line place, a short intro, the programme and a navy footer band with the call to action and the web address. The document has a second, empty page, as the demo makes it today. The Pages panel, the Properties panel (size, margins, six columns, bleed) and the frame edges show the layout.

## Recorded engine answers

FormDesign's layout core is native code that the engine runs and is not built for the browser, so this page replays recorded engine answers: `assets/session.js` (about 0.7 MB) holds what the real engine answered the real window while this demo opened and every panel of every workspace was opened, and pictures of both pages drawn by the engine at 100% and 200%, with absolute paths and user names removed. `demo/record.py` made it; `demo/src/shim/api.ts` replays it.

Live: the window, menus, panels and tabs, workspaces, Home, themes and languages; moving through the pages, zoom and Fit Spread. Replayed: the document state and what the panels show, as recorded; zoom past 200% enlarges the 200% picture. Needs the engine: clicking or typing on the page, selecting, every command that edits, Undo, Save, Export, Print, Make, the assistant, Claude Code, the terminal, font previews, the overprint, separations and proofing views and opening a second copy of a demo. These answer with the app's own error toast that the action needs the FormDesign engine.

## Rebuild

From `frontend/`, `npm run demo` builds all three pages. To record this one again, start the engine and the window with an empty data folder of their own and run `python3 demo/record.py event demos/event/assets/session.js`; `demo/README.md` has the details.

## Licences

The bundle contains the app's own code (Apache-2.0), Inter and JetBrains Mono (OFL 1.1, via @fontsource) and xterm.js (MIT), all listed in `NOTICE.md`. See `CREDITS.md`.
