---
name: formdesign-template-creator
description: "Designs, validates and saves new FormDesign layout templates as JSON specs. Use whenever someone asks to create, add, design, clone, adapt or fix a FormDesign template: a new magazine, newsletter, report, flyer, poster, menu, card, ad size, landing page, email or slide layout; a house style or brand template; 'make a template like this PDF or screenshot'; or 'FormDesign has no template for X'. A template carries page setup, theme, style kit, parents, frames with named slots and continuation pages. Not for filling an existing template with content (formdesign-harness)."
license: Apache-2.0
metadata:
  author: nodemode-ai
  compiled: "2026-10-07"
  version: "0.6.0"
---

# FormDesign template creator

A FormDesign template is one JSON file that the engine compiles into layout commands. Because it is data, a template written once is re-skinned by any brand kit, filled by the Make pass, ranked for briefs that fit it, and edited by hand or by an agent. This skill turns a request ("a menu template for a ramen bar", "our quarterly report style", "something like this PDF") into a template that renders like a professional's work and fills cleanly.

What goes wrong without it: frames placed by eye in points that do not add up, text that overflows its frames at real lengths, hex colours that ignore brands, slots the Make pass does not know, and a template nobody checked by looking at it.

## Procedure

1. **Pin the brief.** Category, kinds the Make pass should rank it for, medium (print with bleed, screen, or web), size and unit, pages, and the content it must hold (how many images, which text slots, a running story or not). If the person gave a reference (PDF, image, URL screenshot), study its structure, not its artwork: grid, hierarchy, proportions. Templates must be original.
2. **Start from the nearest built-in.** `list_templates {category}` and `get_template {id}` show slots; `references/spec.md` has the format and a minimal example. Copy structure ideas, never ids.
3. **Lay out on a grid.** Pick margins, columns and gutter first and place frames on them; derive every rect from that arithmetic. `references/design-rules.md` has sizes, hierarchy and medium-specific rules.
4. **Name content by slots.** Every frame that holds content gets a slot from `references/spec.md` (or `flow: "body"` for running text with a continuation page recipe); decoration gets none. Write original sample text that is the length real content will be.
5. **Style through the kit.** Use the kit's paragraph styles by name; add template styles with `basedOn` and `leading` given with `size`. Colours only through palette roles.
6. **Validate and look.** `validate_template {spec}` (MCP) or `python3 scripts/check_template.py file.json --renders out/` returns problems, pages, slots, overset outside flows, preflight and page images. Look at every image. Fix and repeat until `references/checklist.md` passes.
7. **Save.** `save_template {spec}` (or POST /api/templates). It appears in the gallery and in `list_templates`, and the Make pass ranks it.

## Gates

- `validate_template` reports `ok: true`, no problems, an empty `overset` list, and no preflight errors.
- Every page image was looked at, and the checklist in `references/checklist.md` passes item by item.
- A real `make_layout` with a matching pack fills it without leaving sample text in slots the pack covers.

## Reference files

- `references/spec.md`: the file format, items, slots, styles, themes, kits, kinds and a minimal complete example.
- `references/design-rules.md`: grids, type sizes per medium, hierarchy, image frames, safe areas, and what makes a template look professional.
- `references/checklist.md`: the pre-save checklist.
- `scripts/check_template.py`: validates and renders a template file through the running engine (`--engine http://127.0.0.1:8196`), writing page PNGs.
- `EVALS.json`: evaluations and trigger set.
