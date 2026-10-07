---
name: brand-stylist
description: "Applies a brand to formdes layouts: brand kits (palette roles, font roles, logo), brief colours, and re-skinning an existing template or document."
---

# Brand stylist

## Purpose

Makes every template wear the person's brand in one step, and hands back a brand name to pass to `make_layout` or `new_document`.

## Preconditions

None for a saved brand. Colours and fonts from the person (hex codes, a style guide, a logo file).

## Procedure

1. **Look first.** `list_brands`. If the person's brand exists, pass its name as `brand` to make_layout.
2. **Make a kit.** `save_brand {name, palette, fonts, logo}`. Palette roles: `ink` (text), `paper` (page), `accent` (the brand colour: buttons, rules, kickers), `accent2` (a second colour), `muted` (secondary text), `tint` (light panels), `dark` (dark panels and bands), `light` (text on dark). Font roles, each `[family, style]`: `display` (headlines), `text` (body), `accent` (labels, kickers, buttons), `mono`. Leave out what the brand does not specify: the template's theme fills the rest.
3. **Fonts that exist.** Template fonts come from the bundled library (Inter, Source Serif 4, Source Sans 3, JetBrains Mono and about thirty open families such as Fraunces, Playfair Display, Newsreader, DM Sans, Space Grotesk, Montserrat, Work Sans, Archivo, Oswald, Bebas Neue, EB Garamond, Cormorant Garamond, Libre Baskerville, Lora, Merriweather, Poppins, Raleway, Figtree, Outfit, Syne, Manrope, IBM Plex Sans and Mono) plus the person's system fonts. An unknown family falls back by role; say so when it happens.
4. **Without a kit.** Colours in the brief already re-skin a Make (first mid-tone accent, second accent, darkest dark, light paper when the brief mentions paper). Say "use template colours" by passing `brand: {"name": "none", "palette": {}}` only when the person wants the template as designed.
5. **Re-skin an existing document by remaking it.** Make takes well under a second: run `make_layout` again with the same pack and template plus the brand (or `new_document {template, brand}` for an empty one), then re-apply any set_slots edits. Creating a swatch with an existing name makes a new swatch ("Accent 3") rather than changing the old one, so recolouring in place means creating a new swatch and moving objects to it with `find.changeObjects {fill: "Accent", change: {fill: "New Accent"}}`, which does not reach text colours set by styles.

## Gate

A preview shows the brand colours on accents and panels and the brand fonts on headlines; the logo slot shows the logo, not sample art.

## Hands back

A brand name (or inline brand object) for make_layout.

## Owned resources

None of its own.
