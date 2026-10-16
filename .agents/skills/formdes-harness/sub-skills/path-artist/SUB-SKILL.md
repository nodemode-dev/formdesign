---
name: path-artist
description: "Draws and reshapes vector shapes in formdes with Illustrator-style path commands through run_commands: rounding corners at chosen anchor points, offset paths, outline strokes, the ten Pathfinder operations, simplify, smooth and average."
---

# Path artist

## Purpose

Turns a request about shapes ("round the top corners", "give the logo an outline", "cut the circle out of the square", "clean up this traced path") into the few path commands that do it, so the result stays editable and each step is one undo.

## Preconditions

A document is open and the shapes exist. Get their ids from `outline`; `inspect` with fields `["corner"]` shows the current corners. Every command below runs through `run_commands` as `{command, params}`; `ids` may be left out to act on the selection. `list_commands` with q `path` or group `path` lists them with their parameters and a "Use when" sentence.

## The commands

| Need | Command | Key parameters |
| --- | --- | --- |
| Round some or all corners, kept live while the anchors are edited | `path.roundCorners` | `radius` (pt), `kind` round, invertedRound, chamfer or inset, `anchors` (indices in path order; omit for every corner or the anchors the person picked) |
| The same radius on every corner, InDesign style (any path, polygon or frame) | `object.cornerOptions` | `shape` rounded, inverseRounded, inset, bevel, fancy, none; `size` |
| Bake corners into plain points | `path.expandCorners` | none |
| A parallel outline outside or inside | `path.offset` | `offset` (negative insets), `join` miter, round or bevel, `copy` (default true makes a new shape) |
| A stroke as a filled shape | `path.outlineStroke` | none; a filled object keeps its fill and loses only the stroke |
| Combine shapes | `object.pathfinder` | `op` add, subtract, intersect, excludeOverlap, minusBack, divide, trim, merge, crop, outline (two or more closed shapes, back to front in stacking order) |
| Fewer points, same shape | `path.simplify` | `tolerance` (pt), `cornerAngle`, `straightLines` |
| Turn corner points into smooth ones | `path.smooth` | `amount` 0 to 100 |
| Line points up | `path.average` | `axis` horizontal (one y), vertical (one x), both |

Anchor indices count anchors subpath by subpath, starting at the path's first point; a rectangle's are top left, top right, bottom right, bottom left.

## Order of work

1. Shape first (draw or convert with `object.convertShape`), then corners, then offsets or outlines, then Pathfinder. Pathfinder and Outline Stroke flatten curves to fine segments, so round corners before them.
2. One batch per idea: `run_commands` with all the steps, so the person undoes the idea in one step.
3. `look` at the page afterwards; corners larger than the sides are limited automatically, so check the result rather than computing it.

## Gotchas

- Ellipses and lines have no corners; `path.roundCorners` refuses them.
- Editing anchors keeps live corners only while the anchor count stays the same; adding or deleting a point clears them.
- `path.offset` with a negative offset larger than the shape leaves nothing and is refused.
- In the window the person can also drag the corner handles shown under Direct Selection (Alt-click cycles the kind); tell them so when they want to fine-tune by hand.

## Hands back

The ids of the changed or new shapes, and a `look` of the page.
