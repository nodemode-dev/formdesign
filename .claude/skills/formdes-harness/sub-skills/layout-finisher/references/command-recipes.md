# Command recipes

Each recipe is one `run_commands` batch, which is one undoable step. Ids and bounds come from `get_document` (each item: `id`, `kind`, `slot`, `b` = `[x0, y0, x1, y1]` in points on its spread; spreads are listed in order). Coordinates are spread coordinates: on a facing spread the right page starts at the left page's width. Later steps can use an earlier step's result as `$N.id` (N counts from 0 within the batch). Every recipe here was run against the engine; for anything else use `find_commands`.

## Contents

- Move, resize, scale, rotate
- Colour, opacity, corners, shadow
- Styles and text
- New frames, images and text
- Pages, guides, margins
- Arrange and delete
- Find and change

## Move, resize, scale, rotate

```json
[{"command": "transform.move", "params": {"dx": 0, "dy": -40, "ids": [11]}}]
[{"command": "transform.resize", "params": {"from": [72, 560, 1008, 820], "to": [72, 560, 908, 820], "ids": [11]}}]
[{"command": "transform.scale", "params": {"sx": 0.9, "sy": 0.9, "ids": [11]}}]
[{"command": "transform.rotate", "params": {"angle": 5, "ids": [14]}}]
```

`transform.resize` takes the current bounds as `from` and the wanted bounds as `to`; add `"content": true` to scale an image with its frame.

## Colour, opacity, corners, shadow

```json
[{"command": "object.fill", "params": {"swatch": "Accent", "tint": 0.2, "ids": [11]}}]
[{"command": "swatch.create", "params": {"name": "Brand Teal", "color": "#0f766e"}},
 {"command": "object.fill", "params": {"swatch": "Brand Teal", "ids": [14]}}]
[{"command": "object.stroke", "params": {"swatch": "Ink", "weight": 1.5, "ids": [14]}}]
[{"command": "object.opacity", "params": {"opacity": 0.8, "ids": [11]}}]
[{"command": "object.cornerOptions", "params": {"shape": "rounded", "size": 24, "ids": [9]}}]
[{"command": "object.dropShadow", "params": {"on": true, "distance": 6, "size": 12, "opacity": 40, "ids": [14]}}]
```

Template swatches are named Ink, Paper Color, Accent, Accent 2, Muted, Tint, Dark and Light. Prefer changing a swatch's colour (a brand kit, see `brand-stylist`) over recolouring objects one by one.

## Styles and text

```json
[{"command": "selection.set", "params": {"ids": [12]}}, {"command": "style.paragraph.apply", "params": {"name": "Ad Title"}}]
[{"command": "style.paragraph.edit", "params": {"name": "Body", "chars": {"size": 10.5, "leading": {"kind": "points", "value": 14}}}}]
[{"command": "style.paragraph.edit", "params": {"name": "Title", "chars": {"fontFamily": "Fraunces", "fontStyle": "Black"}}}]
[{"command": "object.textFrameOptions", "params": {"columns": 2, "gutter": 14, "ids": [30]}}]
[{"command": "object.textFrameOptions", "params": {"verticalJustification": "bottom", "ids": [11]}}]
```

Selecting a text frame and applying a paragraph style styles its whole story. Editing a style changes every paragraph that uses it, which is how to resize all body text at once. For replacing words, prefer `set_slots`; for a story by id, `story.setText {story, text}`.

## New frames, images and text

```json
[{"command": "frame.create", "params": {"spread": 0, "rect": [700, 80, 1000, 280], "content": "graphic"}},
 {"command": "file.place", "params": {"path": "/abs/photo.jpg", "frame": "$0.id"}},
 {"command": "object.fit", "params": {"mode": "fillProportionally", "ids": ["$0.id"]}}]
[{"command": "frame.create", "params": {"spread": 0, "rect": [72, 300, 600, 380], "content": "text", "text": "New line of copy"}},
 {"command": "style.paragraph.apply", "params": {"name": "Body"}},
 {"command": "object.label", "params": {"label": "slot:note", "ids": ["$0.id"]}}]
```

Labelling a new frame `slot:NAME` makes it addressable by `set_slots` afterwards. Fitting modes: fillProportionally, fitProportionally, fitContentToFrame, centerContent, fitFrameToContent.

## Pages, guides, margins

```json
[{"command": "layout.pages.insert", "params": {"count": 2}}]
[{"command": "layout.pages.insert", "params": {"count": 1, "after": 0, "parent": "A"}}]
[{"command": "layout.pages.delete", "params": {"pages": [3]}}]
[{"command": "layout.pages.applyParent", "params": {"pages": [2, 3], "parent": null}}]
[{"command": "layout.marginsAndColumns", "params": {"margins": {"top": 54, "bottom": 60, "inside": 54, "outside": 42}, "columns": 3, "gutter": 14}}]
[{"command": "guide.add", "params": {"orientation": "vertical", "position": 540}}]
```

Page indices count from 0. A page added after a threaded story's last frame does not continue the story by itself: `make_layout` adds continuation pages with threaded frames; to extend a story manually, create a text frame on the new page and thread it in the window (or remake with a template that has continuation pages).

## Arrange and delete

```json
[{"command": "object.arrange", "params": {"to": "front", "ids": [11]}}]
[{"command": "object.group", "params": {"ids": [11, 12, 14]}}]
[{"command": "edit.clear", "params": {"ids": [10]}}]
```

## Find and change

```json
[{"command": "find.change", "params": {"find": "Northlight", "change": "Southlight", "scope": "document"}}]
[{"command": "find.changeObjects", "params": {"fill": "Accent", "change": {"fill": "Accent 2"}}}]
```
