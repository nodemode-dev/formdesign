---
name: layout-finisher
description: "Takes a formdes Make result from about 80 percent to done: reading the report, replacing sample text and placeholder images through slots, and structural changes through layout commands."
---

# Layout finisher

## Purpose

Closes the gap between what Make produced and what the person wants, with the fewest calls, and hands back a layout whose report and preview are clean.

## Preconditions

An active document made by make_layout (or opened .fdes) and its report.

## Procedure

1. **Read the report in this order.** `kept` (slots still showing the template's sample text), `placeholders` (image frames with sample art), `emptied` (slots cleared because the content had none, often captions), `fit` (copy trimmed or shrunk to fit), `warnings` (a story longer than the template holds), `unused` (images and articles not placed), `preflight`.
2. **Fill by name.** `set_slots {values: {slot: value}}` in one call for everything you can write: text as a string or a list of paragraphs, images as a pack image id (`img3`) or an absolute path, the running story as markdown under `flow:body`. `get_slots` shows names and current text when the report is not enough. Brand-like slots (brand, nav, footer, url, legal) are often kept: write them from the brief or ask.
3. **Place unused material deliberately.** An unused image can replace a placeholder (`set_slots {hero: "img4"}`); an unused article can go in a feature or item slot, or tell the person it did not fit this template.
4. **Fix fit.** If a headline was shrunk or trimmed, write a shorter one with set_slots rather than accepting a small size. If the body story is longer than the template holds, either cut copy (set flow:body to a shorter markdown) or choose a template with continuation pages.
5. **Structural changes with run_commands.** One batch per change so it is one undo step. Ids and bounds come from `get_document` (items with `id`, `slot`, `b` = bounds in points on the spread). Common recipes are in `references/command-recipes.md`; anything else: `find_commands {query}`.
6. **Clean up and restyle across the document with find_change**, not story by story. `op: count` first, then `op: change_all` (one undo step). `op: queries` lists the built-in fixes (straight quotes to curly, hyphens in number ranges to en dashes, double spaces, stray returns); run one with `query`. To restyle everything in a style, leave `find` empty and set `findFormat {paragraphStyle}` and `changeFormat`. GREP uses these codes (`\r` return, `~=` en dash, `$1` in the change).
7. **Reuse saved pieces with library.** `library {action: "list"}` shows the person's libraries; `items` lists a library's pieces (a logo lockup, a footer, a pull-quote box); `place {name, index, x, y}` drops a copy with its styles and swatches. When you build something the person will want again, `add` it with a title.
8. **Look after each meaningful change** with `preview`.

## Gate

A preview after the last change shows no sample text where the person's content belongs, images cropped on their subjects, and nothing overflowing except a story the person agreed to shorten.

## Hands back

The corrected document and a list of anything still pending from the person (missing logo, prices, a shorter headline they should approve).

## Owned resources

- `references/command-recipes.md`: tested command batches for moving, resizing, recolouring, restyling, adding pages, frames and images, arranging and deleting.
