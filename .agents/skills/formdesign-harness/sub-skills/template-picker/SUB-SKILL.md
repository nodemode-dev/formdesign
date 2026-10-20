---
name: template-picker
description: "Chooses the FormDesign template for a request: reading the ranking, the 71 built-ins by category, size and slots, and when to override the top pick."
---

# Template picker

## Purpose

Returns one template id (or two or three for variations) that fits the content, the kind, the size and the tone.

## Preconditions

A pack from intake, whose summary includes `ranked` (id, score, why).

## Procedure

1. **Trust a ranking that says why.** `why` lists the matched kind, size, tone and signals such as long text. A top pick whose `why` contains the kind the person asked for is usually right.
2. **Override when the kind or size is wrong.** If the person asked for a square ad and the top pick is a story, `list_templates {kind: "ad-social"}` and choose by size. If the article is long (more than about 600 words) prefer templates with a `body` flow: they add continuation pages. If the person has many photographs prefer image-led templates (photo essay, portfolio, lookbook, catalogue); with no photographs avoid templates whose slots are mostly images.
3. **Check slots against content.** `get_template {id}` lists slots. Templates with stat, feature, item or event slots need that kind of content (numbers, sections, menu items, dates); without it they keep sample text. Pick one whose slots the pack can fill, or plan to supply fills.
4. **Variations.** For "show me options", ask make_layout for `variations: 3`; it makes the next best templates as separate documents.
5. **User templates** appear in `list_templates` beside the built-ins and win ties.

## Gate

The chosen template's size and kind match the request, and its slots can be filled from the pack or from fills you are about to pass.

## Hands back

Template id or ids.

## Owned resources

Shared `references/templates.md` (the catalogue with slots) and `references/slots.md`.
