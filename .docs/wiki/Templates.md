# Templates

A template is one JSON file: page setup, a theme and a style kit, parent pages, pages of frames, named slots for the Make pass, and the recipe for continuation pages. The engine compiles it into layout-core commands, so templates hold no code and agents can write them. The full format is `docs/TEMPLATES.md`; this page is the map.

## The library

| Category | Templates | Examples |
| --- | --- | --- |
| Magazine | 9 | bold cover, contents grid, feature spread, interview spread, photo essay, Swiss long read, department news, reviews, back page column |
| Newsletter | 5 | studio update, community bulletin, company quarterly, nonprofit four-page, school term |
| Book | 6 | novel interior, non-fiction chapter, cookbook spread, picture book, poetry chapbook, photo monograph |
| Report | 7 | annual review, business proposal, case study, white paper, research brief, one-page summary, tall infographic |
| Print | 10 | bifold and trifold brochures, product catalogue page, local business flyer, cafe board and restaurant menus, postcard, event, exhibition and gig posters |
| Stationery | 8 | business card, letterhead, invoice, resume, certificate, invitation, event ticket, gift voucher |
| Ad | 12 | social squares, portrait product, story, landscape link ad, sale promo, event announcement, leaderboard, medium rectangle, skyscraper and mobile banners, magazine page |
| Web | 9 | landing page, long-read article, blog journal, docs reference, email digest, conference event, portfolio grid, press release, product detail |
| Presentation | 5 | dark keynote, lookbook, pitch deck, quarterly review, workshop steps |

`./formdes templates` lists them; `./formdes templates [id]` shows one with its slots. In the window, File > New from Template opens the gallery.

## Where things live

| Path | What lives there |
| --- | --- |
| `backend/app/templates/library/[category]/[id].json` | the built-in templates |
| `backend/app/templates/library/_themes.json` | 18 themes: palettes and font roles |
| `backend/app/templates/library/_kits.json` | 5 style kits: paragraph and character styles by role |
| `backend/app/templates/spec.py` | loading, resolving styles, the card the gallery and ranking use, the linter (`problems`) |
| `backend/app/templates/compile.py` | the compiler to core commands: swatches through the colour engine, styles, parents, frames, slots, flows |
| `backend/app/templates/registry.py` | building a template into a document, sample fills, checking a spec |
| `backend/app/templates/brands.py` | brand kits in `~/Formdes/Brands` |
| `~/Formdes/Templates/` | your own templates, any subfolder; they appear beside the built-ins |

## Slots and flows

A frame with `"slot": "title"` is labelled `slot:title` in the document; the Make pass fills it and agents read and set it by name (`get_slots`, `set_slots`). A frame with `"flow": "body"` is labelled `flow:body` and threaded to the other frames of that flow; the Make pass pours the article into it and adds continuation pages. Slot names are a shared vocabulary (title, kicker, subtitle, byline, intro, feature1.title, stat1.label, image1, cta and so on); the table is in `docs/TEMPLATES.md`.

## Brand kits

A brand kit is a palette (accent, accent2, ink, paper, dark, light, muted, tint), font roles (display, text, accent) and a logo. Applied to any template it replaces the theme's colours and fonts. Kits are saved in the Make panel or with the MCP tool `save_brand`; a brief's colours make a one-off kit.

## Adding a template

1. Load the `formdes-template-creator` skill (`.claude/skills/formdes-template-creator/`), which carries the spec, design rules and a checklist.
2. Write the JSON, then check it with the MCP tool `validate_template`, or with the engine running: `python3 .claude/skills/formdes-template-creator/scripts/check_template.py path/to/template.json --renders /tmp/r`. It compiles with sample content, reports problems, overset and preflight, and renders pages to look at. Built-ins can also be checked in bulk with `python -m app.templates.check [id ...] --out /tmp/renders` from `backend/` in its venv.
3. Save it to `~/Formdes/Templates/` (MCP `save_template`) or, for a built-in, to `backend/app/templates/library/[category]/` and run `./formdes agents sync` so the harness skill's template list stays in step.

## Contract

- Coordinates are in the template's own unit, from the page's top-left; the compiler works out spread offsets.
- Every slot has sample text that fits; overset outside a `flow` frame is a bug.
- Samples may hold any text: the compiler routes text that starts with `$` or `${` through tagged placement, because the core's script runner would read it as a step reference. The linter (`spec.problems`) rejects banned characters such as the em dash.
- A new built-in template must pass the linter, the checker and both Make sweeps in [[Verification]].
