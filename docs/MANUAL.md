# formdes manual

This manual is for people laying out documents with formdes. If you are changing formdes's code, the wiki (`.docs/wiki/`) is the place to start.

## Contents

1. [What formdes does](#what-formdes-does)
2. [Starting](#starting)
3. [Making a layout from your files](#making-a-layout-from-your-files)
4. [The workspace](#the-workspace)
5. [Pages and parents](#pages-and-parents)
6. [Text](#text)
7. [Frames, images and links](#frames-images-and-links)
8. [Long documents](#long-documents)
9. [Colour, swatches and brands](#colour-swatches-and-brands)
10. [Tables](#tables)
11. [Rulers and guides](#rulers-and-guides)
12. [Checking your work](#checking-your-work)
13. [Exporting and printing](#exporting-and-printing)
14. [Websites and Publish Online](#websites-and-publish-online)
15. [Books](#books)
16. [Interactive documents](#interactive-documents)
17. [XML](#xml)
18. [Libraries](#libraries)
19. [Layouts from a PDF](#layouts-from-a-pdf)
20. [Working with an agent](#working-with-an-agent)
21. [Files, saving and recovery](#files-saving-and-recovery)

## What formdes does

formdes lays out magazines, newsletters, books, reports, flyers, posters, menus, cards, ads, web pages, emails and slides. You can start from a blank page as in InDesign, but the quickest start is to give formdes your material (an article in Markdown or Word, a PDF, photos, a few lines saying what you want) and let it build a first version from one of its 71 templates. You then finish that version with the tools and panels, or ask an agent to.

## Starting

Open http://localhost:5196 after `./formdes boot`, or open the desktop app. With no document open you see the home screen:

| Button | What it does |
| --- | --- |
| Make a layout (Ctrl+M) | opens the Make panel to build a layout from files and a brief |
| New from template | the template gallery: pick one, then Create empty or Create with sample content |
| Open (Ctrl+O) | a `.fdes`, `.fdoc` or `.idml` document |

Recent documents are listed underneath. File > New Document (Ctrl+N) starts a blank document with print, web or mobile presets, margins, columns and bleed. Dropping files anywhere on the window starts a Make with them.

## Making a layout from your files

The Make panel (right column, or the Make button in the title bar) has three steps.

1. **Content.** Drop files on the box or click it to choose: Markdown, Word, RTF, text, HTML, CSV and spreadsheets, PDFs, images and GIFs. A folder works too.
2. **Brief.** Say what you want in plain words: "a two page newsletter for the Elm Street garden club, warm, green accent". formdes reads the kind of piece, the size (A4, Letter, square, story ad, 1080x1350), the page count, the tone, colours, brand, dates, times and calls to action. Lines like `Headline: The Slow Craft`, `CTA: Read the issue`, `Date: Saturday 12 June`, `Venue: The Old Tram Depot`, `Price: $18`, `URL: harbour.example` fill those slots exactly.
3. **Analyse, then Make layout.** Analyse reads everything and shows the templates that fit, best first, with the reason. Pick one (or Browse all), choose a brand kit or let the brief's colours style it, and press Make layout. Variations 2 or 3 makes alternatives in other templates.

The report under the button says what happened: which slots were filled, which still show the template's sample text, which image frames hold placeholder art, what copy was trimmed or shrunk to fit, which of your files were not used, and preflight problems. The Slots tab lists every named slot with its text or image, so you can replace what is still sample text without hunting for the frame.

Good habits: put the article's title as the first heading, a one-line deck as a heading right under it, and "By Your Name" on its own line; name your photos so their subject is in the file name; state the important facts as `Key: value` lines.

## The workspace

| Area | What is there |
| --- | --- |
| Title bar | menus (File, Edit, Layout, Type, Object, Table, View, Window, Help), undo and redo, zoom, Make, the workspace switcher, Preferences |
| Control bar | the selection's position, size, rotation, fill and stroke; with text, the font, size, leading, tracking and alignment |
| Tool rail | 37 InDesign tools; hold a tool to see its group; single-key shortcuts as in InDesign |
| Document tabs | every open document; the dot marks unsaved changes |
| Canvas | pages on the pasteboard; rulers on the top and left |
| Left of the dock | Properties, Character, Paragraph; Glyphs and Tabs; Changes, Notes and Conditions; States, Buttons and Transitions |
| Right dock | Make and Slots; Swatches, Styles and Table; Pages, Layers and Links; Preflight, Info and Separations; Structure and Tags; Library |
| Status bar | the engine and layout core, the current page, the active tool, the proof condition when Proof Colors is on, the version |

Panel groups can be minimised with the minus button. Drag a group by its header to move it up or down or into the other column; a line shows where it will land. Drag a tab onto another group's header to join that group, or between groups to make a new group of its own. The thin bar between two groups sets the upper one's height (double-click it to let the group size itself), and the bar left of the dock sets the dock's width. Right-click a group's header to close or minimise it; choosing one of its panels in the Window menu brings it back. Tab hides every panel. formdes remembers all of this.

Window > Workspace, or the switcher in the title bar, arranges the panels for a kind of work: Essentials, Advanced, Book, Digital Publishing, Interactive for PDF, Printing and Proofing and Typography. New Workspace saves the arrangement you have under a name of your own, Delete Workspace removes one of yours, and Reset puts the current workspace back as it was saved.

Edit > Keyboard Shortcuts (Alt+Shift+Ctrl+K) changes the keys. Choose a product area or search, pick a command, click New shortcut and press the keys; the dialog says which command has them now, and Assign moves them to the picked command. Remove takes a key away. The default set cannot be changed, so your first change asks for a name and makes your own set, which you can switch, delete or reset; Show Set saves every shortcut in the set as a text file. Tools are listed too.

Preferences (the gear) sets the theme and accent colour, the measurement units, and shows the commands that connect Claude Code, Codex and Cursor.

## Pages and parents

The Pages panel shows parent pages (masters) at the top and the document's pages below. Drag a parent onto pages to apply it; double-click a page to go to it. The buttons underneath insert, duplicate and delete pages; the size menu changes the page size. Layout > Margins and Columns, Document Setup (Alt+Ctrl+P) and Numbering and Section Options (in the Layout menu) work as in InDesign. Page numbers on parents update on every page.

## Text

- **Typing.** Pick the Type tool (T), drag a frame and type, or click into an existing frame. Selecting the frame's out port and clicking another frame threads the story on.
- **Formatting.** The Character (Ctrl+T) and Paragraph (Alt+Ctrl+T) panels and the control bar set fonts, sizes, leading, tracking, indents, spacing, hyphenation and keep options. The composer is Knuth-Plass, like InDesign's Paragraph Composer.
- **Styles.** The Styles tab switches between paragraph, character and object styles. Click a style to apply it; a plus sign marks local overrides; the buttons make a style from the selection, redefine it, edit its options or delete it. F11 and Shift+F11 open it on paragraph and character styles, Ctrl+F7 on object styles. An object style's Options set its name, fill, stroke, stroke weight and paragraph style, and every object using the style follows; Redefine takes the selected object's look into the style; Delete leaves the objects looking as they did.
- **Glyphs.** The Glyphs panel (Alt+Shift+F11) shows every character of the selected text's font, by set; click one to insert it at the cursor.
- **Tabs.** The Tabs panel (Shift+Ctrl+T) shows the paragraph's tab stops on a ruler as wide as its frame. Click to add a stop, drag to move it, drag it down off the ruler to remove it; set its alignment, position, leader and the character to align on.
- **Story Editor.** Ctrl+Y opens the story as plain text, one paragraph per line, for writing and editing without layout getting in the way. Changes appear in the layout as you type and keep their formatting. The column on the left names each paragraph's style, and a red Overset line marks where the text runs past the last frame.
- **Review.** The Changes panel turns Track Changes on and lists every insertion and deletion with its author and time; accept or reject each one, or all. The Notes panel adds a note at the text cursor and lets you edit, delete or turn notes into text. The Conditions panel makes conditions, applies them to selected text, and shows or hides each one, so one document can hold several versions of its copy.
- **Find/Change.** Ctrl+F opens a window that stays open beside the page. The Text tab finds words (the @ menu inserts tabs, returns, dashes and wildcards such as any digit); GREP takes regular expressions with InDesign's codes (`\r` for a return, `~=` for an en dash, `$1` in Change To for the first group); Glyph finds one character in one font; Object finds frames by kind, fill, stroke, opacity, layer or object style. Find Format and Change Format restyle what is found, and with Find What left empty they find every run in a style. Find All lists matches with their page; click one to go to it. Find Next (Alt+Ctrl+F), Change, Change All and Change/Find work as in InDesign, and Change All is one undo. The Query menu holds ready-made clean-ups (curly quotes, en dashes in number ranges, double spaces, empty paragraphs) and the queries you save.
- **Spelling.** Edit > Check Spelling (Ctrl+I) steps through words not in the dictionary from the cursor, with suggestions; Change, Change All, Skip, Ignore All, or Add the word to the document's dictionary. Edit > Dynamic Spelling underlines misspelled words in red as you work. Edit > Dictionary lists the document's own words and its hyphenation exceptions (`ex~am~ple` breaks only where the tildes are). Text is checked in the language set on it (Character panel, Language): US English is built in, and Edit > Language Dictionaries gets the dictionary for another language (German, French, Spanish, Italian, Portuguese, Dutch, the Scandinavian languages, Polish, Czech, Hungarian, Greek, Russian, Ukrainian, Turkish, UK and Canadian English) from the LibreOffice project, or installs a Hunspell .aff and .dic pair you have. Hyphenation is still English only.
- **Type on a path.** The Type on a Path tool (Shift+T) puts type along any line, shape or path you click; Type > Type on a Path > Options sets which part of the type sits on the path, where it starts, flips it, or removes the type.
- **Long text.** Footnotes and endnotes, cross-references, hyperlinks, text variables, conditional text and tables of contents and indexes are in the Type and Layout menus.

Known limits: drop caps and synthesised small caps are set but do not draw yet; text beside an object with text wrap can set its last line wrongly.

## Frames, images and links

File > Place (Ctrl+D) places an image or text into the selected frame or a new one. Object > Fitting fits content to the frame, the frame to the content, or fills proportionally; Auto-Fit keeps it fitted as the frame changes. The Links panel lists every placed file with its page and effective resolution, and relinks, updates or embeds it. Effects (shadows, glows, feathers, bevels, blend modes, opacity) are in the Object menu and Properties.

Edit > Copy, Cut and Paste work across documents and carry stories, pictures, swatches and styles with the objects; pasting objects into text anchors them there, and text copied in another app pastes in. Paste in Place keeps the position; Paste Into puts a copied picture inside the selected frame. Edit > Step and Repeat (Ctrl+Alt+U) makes rows, columns or a run of copies. Object > Paths joins, opens, closes and reverses paths and makes or releases compound paths; Object > Convert Shape turns a frame into another shape; Object > Pathfinder adds, subtracts, intersects or excludes shapes. Object > Generate QR Code draws a QR code into the selected frame, and running it on a QR code changes its text. Type > Create Outlines (Ctrl+Shift+O) turns a text frame into shapes. Object > Captions > Generate Static Caption puts the picture's alt text, name or file name under it.

## Long documents

Type > Hyperlinks & Cross-References links selected text or an object to a web or email address, a page or a text anchor (New Hyperlink Destination), and inserts cross-references that read the anchor's page or paragraph; Update Cross-Reference refreshes them after the pages change. Bookmarks become the PDF's outline. Type > Text Variables defines and inserts running values (a custom text, the chapter number, a running header from a paragraph style, dates, the file name). Type > Insert Footnote adds a numbered note at the cursor. Layout > Table of Contents collects the paragraphs in the styles you choose, with page numbers after a dotted leader, into the selected frame or a new frame on the last page; Update Table of Contents rebuilds it. Index page references (Ctrl+U) mark a topic at the cursor, and Generate Index writes the index with letter headings. Data merge loads a CSV or tab-separated file, inserts `<<Field>>` placeholders at the cursor, and creates a new document with the pages repeated for each record; label a frame `<<@Photo>>` to fill it with the picture whose path is in the Photo field.

## Colour, swatches and brands

The Swatches panel holds the document's colours, tints, gradients and colour groups; print templates use CMYK swatches converted through the working colour space so blacks print rich. A brand kit (palette, fonts, logo) re-skins any template: make one in the Make panel's Style step, or let a brief's colour words make a one-off kit.

## Tables

Click into text and use the Table panel (Shift+F9) to insert a table, or Table > Convert Text to Table. With the cursor in a table the panel sets header and footer rows, row height, column width, cell fill, inset and vertical alignment, the border, alternating row fills, and table and cell styles, and inserts, deletes, merges and sorts. The Table menu has the same commands.

## Rulers and guides

Rulers count from the top-left of the spread in your units (Preferences, Units). Drag from a ruler onto a page to place a guide there; drop it on the pasteboard, or hold Ctrl while dropping, for a guide across the whole spread. View > Rulers (Ctrl+R) hides them; View > Guides and Margins (Ctrl+;) hides the guides. Layout > Create Guides makes rows and columns of guides, within the margins or the page; View > Grids & Guides > Delete All Guides on Spread clears them.

## Checking your work

The Preflight panel lists overset text, missing fonts and links, low-resolution images and RGB colour in print documents, and goes green when the document is ready. Choose a profile for where the file is going: Commercial print adds 300 ppi images, 9 pt of bleed on anything that reaches the trim, text at least 9 pt inside the trim, type of 6 pt and up and at most two spot inks; PDF/X-1a adds transparency; Saddle-stitched booklet wants a page count in fours; Digital and screen accepts RGB and 72 ppi. Define makes your own profile from any of them. The Info panel shows the pointer's position and the selection's size. Preview (W) hides guides and frame edges.

View > Separations Preview (Shift+F6) opens the Separations panel. Choose Separations, then Cyan, Magenta, Yellow or Black to see only that plate, as a printer's film would show it, or CMYK for all four. Choose Ink Limit and set a total (300 percent is common for coated stock) to see in red every area that would carry more ink than the press allows; Off returns to the normal view. Spot inks are listed underneath, each with Print as process. View > Overprint Preview (Alt+Shift+Ctrl+Y) shows how objects set to overprint will mix with the colours beneath them.

View > Proof Colors shows the page as it will print on the press chosen in View > Proof Setup: Working CMYK, or Custom, where you pick any CMYK profile (Load Profile adds your printer's .icc file, kept for next time), the rendering intent, and Simulate Paper Color for the paper's own white and the ink's real black. Proof Setup also simulates sRGB, legacy Macintosh RGB and monitor RGB screens, and two kinds of colour blindness. While proofing, the status bar names the condition.

## Exporting and printing

File > Export (Ctrl+E) exports PDF (print or interactive, page ranges, spreads, bleed and crop marks, transparency flattening, PDF/X, tagged PDF), InDesign Markup (IDML), EPUB and fixed-layout EPUB, HTML, a website, PNG, JPEG, text and XML, to a download or to the Exports folder. File > Package collects the document, its links, an IDML copy and a report into a folder for a printer. File > Print prints or writes a booklet.

## Websites and Publish Online

Export's **Website** format turns the open document into a site you can put on any web host: a folder (or a .zip) of pages that reflow for phones, set in the document's own fonts and colours. What it makes depends on the document, and **Web pages** lets you choose:

- **Automatic** picks for you. A magazine, newsletter or report with several headlines becomes a home page with a card for each article and a page for each; a web template becomes one web page per page; anything else becomes one page.
- **One per article** follows the Articles panel when it has articles, otherwise each headline starts an article.
- **One per page** keeps the document's pages, with their rows and columns laid out side by side on a wide screen and stacked on a phone.
- **One long page** puts everything on one page.

**Layout** comes from the template (article, landing page, card, email or docs) unless you choose one. **Edition** adds a page-by-page view of the layout exactly as designed, with spreads side by side, its text searchable and its links clickable; it is on by default for multi-page print layouts. **Address** is where the site will live (for example `https://example.com/issue-12`): with it, the site gets a sitemap, an RSS feed for its articles and links that work when shared. Site title, description and language are read from the document unless you type them.

The result lists what to fix before the site goes out: pictures without a description (Object > Object Export Options, Alt Text), buttons with no link (Window > Interactive > Hyperlinks), placeholder art from Make, and fonts whose licence does not allow putting them on a website (the site uses a similar system font instead). Fonts that can be shared go into the site with their licences.

File > Publish Online gives the open document's site a link you can open now: formdes serves it from this computer, and publishing the same document again updates it. The list below shows every published site; for each you can open it in the browser, copy its link, show its folder, download it as a .zip, or unpublish it (click the bin twice). To put a site on the public web, download its .zip or export the website into a folder, and upload that folder to a static host such as GitHub Pages, Netlify or Cloudflare Pages.

## Books

File > Book collects saved documents as the chapters of one book. Add documents from your Projects folder, put them in order, mark the chapter whose styles lead (the style source), then:

- **Update Numbering**: each chapter starts after the previous one ends.
- **Synchronize Styles**: the style source's paragraph and character styles and swatches are copied to every chapter.
- **Export Book PDF**: one PDF with every chapter in order.

Double-click a chapter to open it. A chapter open with unsaved changes has to be saved before numbering or synchronising.

## Interactive documents

Three panels in the Window menu make a document that does things in an interactive PDF, an EPUB or HTML. **Object States**: select two or more frames and choose New to stack them as the states of one object; click a state to show it, double-click to rename it, and Release to turn the states back into ordinary frames. **Buttons and Forms**: select a frame and choose a type. A button goes to a page or a URL when clicked; a text field, check box, combo box, list box or signature field gets a name, a default value, its choices one per line, a font size (0 for automatic), and whether it is multiline or required. With a placed video or sound selected, the same panel sets Play on page load, Loop and Show controls. **Page Transitions**: choose a transition and a speed for the current spread, or Apply to All Spreads; Clear All removes them.

## XML

The Tags and Structure panels (Window > XML Structure, Alt+Ctrl+1) give a document a structure that other systems can read and fill. Make a tag with New, then select a frame, or some text, and click the tag to make it an element; Untag removes the element and keeps the content. Map styles to tags chooses, for each paragraph style, the element its paragraphs export as and come back in as. Load DTD reads a DTD and adds its elements as tags; Validate in the Structure panel then lists anything that does not follow it. Export writes the structure and its text as XML; Import fills tagged frames from an XML file with matching elements and tells you what had nowhere to go.

## Libraries

A library keeps pieces you use again (a logo lockup, a footer, a pull-quote box) with their styles and colours. Window > Library opens the panel: choose a library or make a new one, select objects on the page and choose Add, and the piece is stored with a thumbnail. Drag a thumbnail onto the page to place a copy where you drop it, or pick it and choose Place to put it back where it was taken from. Double-click to rename; search finds pieces by name. Libraries are `.fdlib` files in `~/Formdes/Libraries`; set `FORMDES_LIBRARIES_DIR` to keep them in a shared folder instead.

## Layouts from a PDF

Drop a PDF on the Make panel or the canvas to lay it out again in a template: formdes reads its structure (title, sections, text) and its images and makes a layout from them, as with any other file. File > Place puts a PDF's pages on the page as graphics, and File > Export writes PDF. formdes does not view or edit PDFs themselves; that is FormPDF, a separate app.

## Working with an agent

Claude Code, Codex and Cursor can make and finish layouts for you through formdes's MCP server; set them up once with `./formdes agents install` (Preferences, Agents shows the commands). Then ask in your own words: "make a two-page magazine feature from article.md and these photos, elegant, cream paper". The agent reads your files, makes the layout, looks at the pages, replaces sample text and exports, and its changes appear live in the open window, where you can keep editing. From a terminal, `./formdes make files --brief "..." --out layout.pdf` does the same without an agent.

## Files, saving and recovery

Save (Ctrl+S) writes a `.fdes` file to `~/Formdes/Projects` unless you choose another place. A `.fdes` keeps the layout, the slot names, and the content it was made from, so it can be made again in another template. Save As can also write `.idml` for InDesign or a bare `.fdoc` layout. Documents saved by formdes before 0.17.0 no longer open; open an IDML export of them instead. Every minute formdes keeps a recovery copy of documents with unsaved changes; after a crash, File > Recover Unsaved Documents reopens them.
