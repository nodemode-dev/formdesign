# formdes templates

A formdes template is one JSON file that describes a document: its page setup, which theme and style kit it uses, its parent pages, its pages of frames, the named slots the Make pass pours content into, and the recipe for continuation pages when a story runs long. The engine compiles it into layout-core commands (`backend/app/templates/compile.py`), so a template never contains coordinates in anything but its own page space and never names a command.

Built-in templates live in `backend/app/templates/library/<category>/<id>.json`; your own go in `~/Formdes/Templates/` (any subfolder) and appear beside them. Themes are in `library/_themes.json`, style kits in `library/_kits.json`.

## Checking a template

```bash
cd backend && . venv/bin/activate
python -m app.templates.check <id> [<id> ...] --out /tmp/renders --size 900
```

It compiles each template with sample content in a private layout core, renders the first pages to PNG and prints one JSON line per template (`ok`, `pages`, `slots`, `overset`, `preflight`). Overset in a `flow` frame is expected (continuation pages are added by the Make pass); overset anywhere else means the sample does not fit. Look at every render before calling a template done.

## The file

```jsonc
{
  "id": "magazine-feature-spread",          // unique, kebab-case, the file name without .json
  "name": "Feature Spread",                 // shown in the gallery
  "category": "magazine",                   // magazine | newsletter | book | report | print | stationery | ad | web | presentation | other
  "kinds": ["magazine"],                    // brief kinds it serves (see Kinds below); the Make pass ranks by these
  "tags": ["editorial", "serif", "spread"],
  "tone": ["elegant", "bold"],              // bold | minimal | elegant | playful | corporate | vintage | tech | warm | dark
  "description": "One sentence on what it is and what it holds.",
  "medium": "print",                        // print (CMYK swatches) | screen (RGB, ads) | web (RGB, exported as HTML)
  "theme": "editorial",                     // a key of _themes.json: palette and fonts
  "kit": "longform",                        // a key of _kits.json: paragraph and character styles
  "scale": 1.0,                             // multiplies the kit's base size
  "palette": {"accent": "#c2410c"},         // optional: override palette roles
  "fonts": {"display": ["Fraunces", "Black"]},  // optional: override font roles
  "styles": {"paragraph": {"Hero Title": {"basedOn": "Title", "size": 3.9}}, "character": {}},
  "page": {"size": "Letter", "unit": "pt", "facing": true, "startLeft": true,
           "margins": {"top": 54, "bottom": 60, "inside": 54, "outside": 42},
           "columns": 3, "gutter": 14, "bleed": 9, "baseline": 13.5},
  "parents": {"A": [ /* items on parent A */ ]},
  "pages": [ {"parent": "A" | null, "items": [ /* items */ ]} ],
  "flow": {"body": {"page": {"parent": "A", "items": [ /* items for each continuation page */ ]}, "max": 24}},
  "web": {"layout": "article"}              // article | landing | card | email | docs: how web export arranges it
}
```

### Page

- `size`: a preset name (`Letter`, `Legal`, `Tabloid`, `A3`, `A4`, `A5`, `A6`, `B5`, `Digest`, `Trade`) or `[width, height]` in `unit`.
- `unit`: `pt` (default), `px` (one px is one pt; use it for screen and web), `in`, `mm`, `cm`, `pc`. Every number in the template (rects, from/to, margins, gutter, bleed, baseline, corner, wrap offsets, insets) is in this unit.
- `orientation`: `landscape` swaps a preset's sides.
- `facing`: spreads with left and right pages. `startLeft`: the first page is a left page, so pages 1 and 2 form a spread (use it for openers).
- `margins`: a number, `[top, bottom, inside, outside]` or the object form. On facing pages inside is the spine side.
- `columns`, `gutter`: the page grid (shown as guides; text frames set their own columns).
- `bleed`: past the trim on every side. `baseline`: the baseline grid increment (print only).

### Items

Every item has a `kind` and, except lines, a `rect`: `[x, y, width, height]` from the page's top-left corner, or the keywords `"page"` (the trim), `"margins"` (the margin box of that page side) or `"bleed"` (the trim plus bleed on the outer edges).

| kind | what it makes | keys |
| --- | --- | --- |
| `text` | a text frame | `style` (a paragraph style), `slot` or `flow`, `sample` (a string, or a list for several paragraphs), `text` (literal; `{page}` or `<page>` is the current page number, `{next}` and `{prev}` the next and previous page numbers), `columns`, `gutter`, `inset` (number or `[t, l, b, r]`), `valign` (`top`, `center`, `bottom`, `justify`), `fill`, `ignoreWrap` |
| `image` | a graphic frame | `slot`, `fit` (`fill` default, or `fit` for logos), `shape: "ellipse"`, `corner`, `stroke` |
| `rect`, `ellipse`, `polygon` | a shape | `fill` (a palette role or `#rrggbb`), `tint` (0 to 1), `gradient`, `stroke`, `corner`, `sides` (polygon) |
| `line` | a rule | `from: [x, y]`, `to: [x, y]`, `stroke` |

Shared keys: `opacity` (0 to 1), `blend`, `rotate` (degrees), `wrap` (`box`, `contour`, `jump`, `column`) with `wrapOffset`, `shadow` (`true` or `{distance, size, opacity}`), `mirror` (on facing pages, the item flips to the other side on left pages), `label`, `name`, `nonprinting`. On parents, `side` is `left`, `right` or `both` (default).

- `stroke`: `{"color": "accent", "weight": 1.5, "type": "dashed"}`.
- `gradient`: `{"angle": -90, "stops": [["dark", 0, 0.0], ["dark", 100, 0.8]]}`: each stop is `[role or #hex, location 0 to 100, opacity 0 to 1]`; angle -90 runs top to bottom.
- Items are stacked in the order written: the first is at the back.
- A non-white `paper` palette role puts a paper-coloured rectangle at the back of every page automatically.

### Slots

A slot names what a frame is for, so the Make pass and agents can fill it (`slot:<name>` is the frame's script label). Use these names; the Make pass knows them:

| Slot | Filled with |
| --- | --- |
| `title`, `subtitle`, `kicker`, `byline`, `date`, `issue` | the headline, the deck or standfirst, the section or category label, `By <author>`, the date, the issue line |
| `intro`, `summary` | the first paragraph, or a summary of up to about 60 words |
| `section` | the first section heading |
| `pullquote`, `quote`, `quotecredit` | a strong sentence from the text, and who said it |
| `caption`, `caption1`... `credit` | image captions and credits |
| `cta`, `cta2`, `note`, `url`, `footer`, `legal`, `contact`, `address`, `phone`, `email` | calls to action and small print from the brief |
| `brand`, `tagline`, `nav` | the brand name, its line, the navigation |
| `name`, `role`, `details` | a person's name, title and contact lines (cards, résumés, certificates) |
| `list` | the first bullet list |
| `stat1`, `stat1.label`... | a number from the text and what it counts |
| `feature1.title`, `feature1.body`... | the first sections or list items, title and one paragraph each |
| `item1.title`, `item1.body`, `item1.price`... | menu, catalogue and product entries |
| `eventdate`, `eventtime`, `location` | event details |
| `toc` | a generated table of contents |
| `hero`, `image1`... `image9`, `portrait`, `product`, `background`, `gallery1`... | pictures, best fit by orientation and role |
| `logo` | the image whose role is logo (use `"fit": "fit"`) |
| `closing`, `runninghead` | a closing line before a final call to action, the running head |
| `section1`, `kicker1`... | repeated section heads and labels, in order |
| `item1.date`, `item1.time`, `item1.qty`, `item1.rate`, `feature1.page`, `feature1.meta` | per-entry details: dates, times, quantities, rates, page references, meta lines |
| `subtotal`, `tax`, `total`, `duedate` | invoice totals and terms |

The same slot name may appear on several frames (a brand name in the header and the footer); all of them receive the same content. A slot the Make pass has no content for keeps its sample text, and the report lists it under `kept`, so the reviewer knows what is still placeholder.

`flow` names a story that runs on (`"flow": "body"` for the article). All frames with the same flow on the template's pages are threaded in order, and their samples join into one story for previews. When the story oversets, the Make pass adds pages from `flow.<name>.page` until it fits or `max` pages are added; `"max": 0` means the story must fit the template's own pages.

### Authoring notes

- The first baseline sits at the font's ascent, so a one-line frame needs about 1.3 to 1.5 times the point size in height (display faces such as Oswald, Bebas Neue, DM Serif Display need the upper end).
- Give `leading` with `size` when a style changes size a lot; otherwise the inherited multiplier is applied to the new size.
- Bulleted and numbered styles get a tab stop at their `leftIndent`, so use a hanging indent (`leftIndent: 1.2, firstLineIndent: -1.2`) and wrapped lines align. `tabs` sets explicit stops in ems: `[8]` or `[{"position": 8, "align": "right", "leader": "."}]`.
- Prices and figures may start with a currency sign; the compiler routes such text safely.
- Display styles (display font, or 1.5 times the base size and up) do not hyphenate unless the style says so.
- Drop caps and true small caps depend on the layout core: `dropCap` is stored on the style and exported to IDML; `smallCaps` is set as tracked capitals.
- Stroke `type` is `solid`, `dashed` (optional `dash` and `gap` in points) or `dotted`.

### Styles

The kit supplies these paragraph styles; use them by name, override them, or add your own in `styles.paragraph` with `basedOn`:

`Body`, `Body First` (no indent, a drop cap in longform), `Body No Indent`, `Title`, `Subtitle`, `Kicker`, `Byline`, `Heading 1`, `Heading 2`, `Heading 3`, `Quote` (pull quote), `Blockquote`, `Caption`, `Credit`, `List`, `Numbered`, `Code`, `Folio`, `Running Head`, `Label`, `Small`, `CTA`, `Stat`, `Stat Label`, `Table Head`, `Table Body`; `display` and `doc` also have `Price`; `web` also has `Nav`. Character styles: `Strong`, `Emphasis`, `Link`, `Accent`, `Code`.

Style attributes, in kit units:

| key | meaning |
| --- | --- |
| `font` | a font role: `display`, `text`, `accent`, `mono` |
| `fontStyle` | `regular`, `bold`, `italic`, `boldItalic` (resolved against the role's family); or `style` for an exact face name |
| `size` | times the kit's base size |
| `leading` | times the size |
| `tracking` | thousandths of an em |
| `color` | a palette role |
| `caps`, `smallCaps`, `underline` | booleans |
| `align` | `left`, `center`, `right`, `justify`, `fully`, `towardsSpine`, `awayFromSpine` |
| `firstLineIndent`, `leftIndent`, `rightIndent` | ems of the size |
| `spaceBefore`, `spaceAfter` | times the base size |
| `dropCap` | lines; `dropCapChars` characters |
| `keepWithNext`, `hyphenate`, `balance`, `grid` | keeps, hyphenation, balanced ragged lines, align to the baseline grid |
| `list` | `bullets` or `numbers` |
| `ruleAbove`, `ruleBelow` | `{weight, color, offset}` |
| `shading` | `{color, tint, offset}` |
| `basedOn`, `next` | inheritance and the next style |

### Themes and kits

Palette roles: `ink` (text), `paper` (page), `accent`, `accent2`, `muted`, `tint` (light panels), `dark` (dark panels), `light` (text on dark). They become swatches named Ink, Paper Color, Accent, Accent 2, Muted, Tint, Dark and Light. Font roles: `display`, `text`, `accent`, `mono`, each `[family, style]`. A brand kit replaces palette roles and font roles, so every template re-skins to a brand in one step; keep colours on roles, never `#hex` on text, so that works.

Themes: editorial, swiss, modern-serif, classic, bold-sans, geometric, tech, warm, luxury, news, poster, playful, corporate, literary, retro, studio-dark, fresh, condensed-news. Kits: longform (base 9.5 pt), web (base 18 px), display (base 12), doc (base 9.5), news (base 9).

Fonts available to templates: the families in `backend/app/fonts/fonts.json` (built by `./formdes install-fonts`) plus Inter, Source Sans 3, Source Serif 4 and JetBrains Mono inside the layout core. An unavailable family falls back by role.

## Kinds

The brief parser (`backend/app/intake/brief.py`) recognises: `magazine`, `newsletter`, `book`, `report`, `brochure`, `flyer`, `poster`, `postcard`, `menu`, `catalog`, `card`, `letterhead`, `invoice`, `resume`, `certificate`, `invitation`, `ticket`, `slides`, `infographic`, `one-pager`, `ad-social`, `ad-story`, `ad-banner`, `ad-print`, `web-landing`, `web-blog`, `web-article`, `web-portfolio`, `web-product`, `web-event`, `web-email`, `web-press`, `web-docs`.

## Rules for library templates

- Original work only: no layouts traced from Adobe or anyone's templates, no real brand names, logos or trademarks, sample copy written for the template.
- No emoji and no em dashes in any text; use a middle dot or a comma.
- Every frame that holds content has a slot or a flow; decoration has none.
- Samples must fit their frames at the kit's sizes (no overset outside flows).
- Use palette roles, not hex, for anything a brand should recolour.
