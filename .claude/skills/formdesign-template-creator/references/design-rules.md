# Design rules for FormDesign templates

What separates a template that looks professional from one that looks assembled. Each rule has a reason; break one only knowingly.

## Contents

- Grid first
- Type sizes by medium
- Hierarchy
- Frames for real content
- Images
- Colour
- Medium-specific rules
- Common faults found by the checker

## Grid first

Choose margins, columns and gutter before placing anything, then compute rects from them: column width = (page width minus inside and outside margins minus gutters) divided by columns. Frames start and end on column edges or on the margin box; vertical positions sit on the baseline increment for print (13.5 pt baseline with a 9.5 pt body is a sound default). Rects that do not add up are the main reason an otherwise good layout looks off. On facing pages, give the inside margin room for the binding (at least as large as the outside one on a perfect-bound magazine).

## Type sizes by medium

| Medium | Body | Captions, small print | Headlines |
| --- | --- | --- | --- |
| Magazine, newsletter, report (pt) | 9 to 10.5 | 7 to 8 | 28 to 60, display on openers up to 120 |
| Book interior (pt) | 10 to 11.5 | 8 | chapter heads 18 to 30 |
| Poster (pt, viewed at distance) | 14 to 24 | 10 to 12 | 120 to 400 |
| Flyer, menu, brochure (pt) | 9.5 to 12 | 7.5 to 9 | 24 to 72 |
| Business card (pt) | 7 to 9 | 6.5 | name 11 to 14 |
| Social ad (px on 1080) | 28 to 40 | 20 to 24 | 80 to 160 |
| Banner ads (px) | 12 to 16 | 9 to 11 | 18 to 32 |
| Web page (px) | 17 to 20 | 13 to 15 | 40 to 80 |
| Slides (px on 1920) | 28 to 36 | 20 to 24 | 64 to 120 |

The kit's base size times `size` gives points or pixels: longform and doc kits are 9.5, news 9, display 12, web 18.

## Hierarchy

One dominant element per page (a headline, a photograph, a number), then at most two further levels. Contrast by size steps of about 1.5 times, by weight, or by colour, not by all three at once. Kickers and labels are small, tracked capitals in the accent role. Keep line lengths of body text between about 45 and 75 characters: use columns on wide pages.

## Frames for real content

Sample text must be the length real content will be: a headline of 6 to 10 words, a deck of 15 to 30, a caption of 10 to 20. A one-line frame needs 1.3 to 1.5 times the point size in height because the first baseline sits at the font's ascent. Leave headroom: a title frame that exactly fits its sample will overflow with a longer real headline. Running text belongs in a `flow` with a continuation page recipe; give it `max` 0 only when the template must stay a fixed number of pages.

## Images

Size image frames to common aspect ratios (3:2, 4:3, 1:1, 4:5, 16:9) so photos crop well; the Make pass matches images to frames by aspect. Full-bleed images use `"rect": "bleed"` on print. Put text over images only with a gradient or a panel behind it (a dark fade from 0 to about 0.85 opacity). Logos get `"fit": "fit"` and a frame with clear space around the mark.

## Colour

Text and fills use palette roles (ink, paper, accent, accent2, muted, tint, dark, light) so brand kits recolour them; `#hex` only for decoration that must never change. Contrast text against its background at least 4.5:1 for body and 3:1 for large display text. A non-white paper role colours every page automatically.

## Medium-specific rules

- **Print:** `medium: "print"`, bleed 9 pt (0.125 in, 3 mm) when anything touches the edge, extend such items to the bleed, keep text at least the margin from the trim.
- **Screen ads:** `medium: "screen"`, `unit: "px"`; keep text out of the outer 5 percent; one message, one call to action; logo in a corner.
- **Banners:** at 728 x 90, 300 x 250, 160 x 600 and 320 x 50 the only content is a short headline, an optional line, a button and a logo; nothing else fits.
- **Web pages:** `medium: "web"`, width 1440 (email 600), tall single page, set `web.layout`; sections as full-width bands with `tint` or `dark` fills.
- **Slides:** 1920 x 1080, 4 to 6 slides as pages, a parent with a footer and a `{page}` number.

## Common faults found by the checker

- Overset in a non-flow frame: the sample is too long or the frame too short (remember ascent height).
- A style that changes `size` without `leading`: inherited leading in points makes lines collide.
- Text starting with a currency sign is fine; text containing an em dash or emoji is rejected.
- A slot name the Make pass does not know keeps its sample forever: use the slot table.
- A dashed rule needs `"type": "dashed"`; dotted renders as round dots.
