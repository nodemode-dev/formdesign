# The Make Pass

The Make pass turns files and a brief into a filled layout in one call, about one second for most templates. It is the same code behind the Make panel, `./formdes make`, `POST /api/make` and the MCP tool `make_layout`. It aims to get a person most of the way and to say exactly what is left, never to invent copy.

## Steps

| Step | Code | What happens |
| --- | --- | --- |
| 1. Intake | `backend/app/intake/` | Each file becomes articles (title, deck, byline, blocks), images (size, orientation, colours, focal point), tables and quotes. The brief becomes kinds, medium, size, page count, tone, colours, brand, subject, call to action and fields. All of it is one content pack with an id. |
| 2. Rank | `backend/app/autolayout/rank.py` | Templates are scored on the stated kinds (or kinds inferred from titles and file names), tone, size, page count, how many images there are, whether the article needs a flow, tag and name words, and penalties for a sale template with no sale or an image template with no images. The top three are returned with the reasons. |
| 3. Build | `backend/app/templates/registry.py`, `compile.py` | The chosen template is compiled to core commands, re-skinned by the brand kit (or the brief's colours), and flows are threaded. |
| 4. Fill slots | `backend/app/autolayout/content.py`, `make.py` | Each `slot:NAME` frame gets its value: brief fields first, then the article (title, deck, byline, intro, sections, quotes, stats, features, items, lists), then the brief's subject or brand. A slot with nothing to say keeps its sample text and is reported. |
| 5. Images | `assign_images` in `make.py` | Images go to image slots by aspect, role and effective resolution; empty image frames get on-palette placeholder art. |
| 6. Flow | `continue_flow`, `tighten` | The article pours into the `flow:` story; while it oversets, a continuation page built from the flow's `page` rule in the template is added and threaded (up to the flow's `max`, 24 by default), then the pass tightens the copy so the last page is not left nearly empty. |
| 7. Fit | `fit_text` | An overset frame drops whole paragraphs or sentences (prose), or shrinks to 55 percent (titles, dates, prices) before any word is cut. |
| 8. Report | `make.py` | `filled`, `kept` (still sample text), `placeholders`, `emptied`, `fit`, `unused` files, preflight, and `next`: the short list of what a person or agent should do. |

## Reading a brief

`backend/app/intake/brief.py` reads a brief without a model. `Key: value` lines (Headline, CTA, Date, Venue, Price, URL and about sixty aliases) become fields whose names are slot names. Then patterns fill what is still missing: email, URL, phone, price, discount, a date (Saturday, 12 June 2026), a time or range (7pm to 11pm), an issue (Spring issue, Issue 12), page counts in digits or words, sizes (A4, 1080x1350, square, story ad, leaderboard), a brand (for the Elm Street garden club; Harbour magazine) and an event name, which becomes the title. Write the important words as fields when they matter.

## Reading an article

A Markdown heading at the top is the title; a heading of five to thirty words right under it is the deck; a short "By Name" line in the first two blocks is the byline. Front matter (`title`, `subtitle`, `author`, `date`, `tags`) wins over all of these.

## Contract

- The pass never writes copy it was not given. Anything it cannot fill stays sample text and is listed in `kept`; agents replace it with `set_slots`.
- Every template in `backend/app/templates/library/` must make cleanly with an article and photos, and with a brief alone. The sweep in [[Verification]] runs both before each release.
- New slot names are added to the slot table in `docs/TEMPLATES.md` and handled in `Content.value` in `backend/app/autolayout/content.py`.
- The report stays small (well under 1,000 tokens) because agents read it on every make.
