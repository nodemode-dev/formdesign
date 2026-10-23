---
name: path-artist
description: "Draws and reshapes vector art in FormDesign with path commands through run_commands: corners at chosen anchor points, offset paths, outline strokes, the ten Pathfinder operations, simplify, smooth and average, Shape Crafter, blends, width profiles, live effects and warps, brushes, Repeat, symbols, Recolor Artwork, Trace Image, Fill Regions, masks, freeform gradients, gradient meshes, envelopes and artboards."
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
| The same radius on every corner (any path, polygon or frame) | `object.cornerOptions` | `shape` rounded, inverseRounded, inset, bevel, fancy, none; `size` |
| Bake corners into plain points | `path.expandCorners` | none |
| A parallel outline outside or inside | `path.offset` | `offset` (negative insets), `join` miter, round or bevel, `copy` (default true makes a new shape) |
| A stroke as a filled shape | `path.outlineStroke` | none; a filled object keeps its fill and loses only the stroke |
| Combine shapes | `object.pathfinder` | `op` add, subtract, intersect, excludeOverlap, minusBack, divide, trim, merge, crop, outline (two or more closed shapes, back to front in stacking order) |
| Fewer points, same shape | `path.simplify` | `tolerance` (pt), `cornerAngle`, `straightLines` |
| Turn corner points into smooth ones | `path.smooth` | `amount` 0 to 100 |
| Line points up | `path.average` | `axis` horizontal (one y), vertical (one x), both |

Anchor indices count anchors subpath by subpath, starting at the path's first point; a rectangle's are top left, top right, bottom right, bottom left.

## Art beyond paths

The art features, each kept live (editable, and undone in one step) unless the row says it makes new objects. Points named `[x, y]` are spread points unless the parameter says shares (0 to 1) of the object's bounds.

| Need | Command | Key parameters |
| --- | --- | --- |
| Merge or erase the regions where shapes overlap | `path.shapeBuilder` | `points` inside the regions, `mode` merge or erase (makes new shapes) |
| Steps between two objects, morphing like shapes | `object.blend` | `steps` or `distance`, `spine` (id of a path to follow), `live` (rebuilds when an end or the spine changes); `object.blend.expand` |
| A stroke that swells and tapers | `object.widthProfile` | `profile` lens, taperStart, taperEnd, bulge, pinch, wave; or `points` [[at, width]] |
| Live effects and warps in the appearance | `object.effect` | `kind` roundCorners, zigZag, roughen, puckerBloat, twist, transform (with copies), warp (13 styles, `bend`); `index`, `remove`, `clear`; `object.expandAppearance` bakes them |
| A brushed stroke | `object.brush` | `kind` calligraphic, scatter, art or pattern (a `symbol` side tile, `start`, `end`, `corner` tiles), none |
| Copies in a ring, grid or mirror that follow the original | `object.repeat` | `kind` radial, grid or mirror; `release`, `expand` |
| Reusable art placed many times | `symbol.create`, `symbol.place` | `name`; `at`, `scale` |
| Recolour art at once | `object.recolor` | `map` [[from, to]], or `hue`, `saturation`, `brightness` shifts |
| A picture into vector shapes | `object.imageTrace` | `mode` blackWhite or colors, `colors`, `detail`, `noise` (makes new shapes) |
| Fill the regions lines and shapes make | `path.livePaintMake` then `path.livePaint` | `points`, `swatch`; fills on a live paint group follow the paths when they change |
| Clip or fade art with a top object | `object.clippingMask`, `object.opacityMask` | `release`; `clip` for the opacity mask |
| Colours placed at points that blend | `object.freeformGradient` | `points` [[x share, y share, colour]], `spread` |
| A mesh of coloured points | `object.mesh` | `cols`, `rows`, `highlight`; `point` [col, row] with `color` or `to` [x share, y share]; `clear` |
| Bend art into a shape or around pins | `object.envelope` | `kind` topObject, mesh or puppet (`pins`); `moves`, `release`, `expand` |
| A gradient along or across a stroke | `object.stroke` | `gradientMode` along or across (with a gradient swatch on the stroke) |
| Export regions on their own (icons, social sizes) | `artboard.create`, `artboard.exportPdf` | `name`, `rect` in spread points (or the selection's bounds); export takes `artboards` and `path`; GET `/api/core/artboard.png?artboard=` gives one as PNG |

In the window the same work has tools: Shape Crafter (Shift+M), Width (Shift+W), the Mesh tool (with the gradient tools: click a shape to add a mesh, drag points, click inside to add lines, Alt-click to remove them) and the Artboard tool (Shift+O: drag to draw, drag inside to move, corners resize, Delete removes). Shift+D cycles Draw Normal, Behind and Inside for new shapes; double-click a group to isolate it.

## Order of work

1. Shape first (draw or convert with `object.convertShape`), then corners, then offsets or outlines, then Pathfinder. Pathfinder and Outline Stroke flatten curves to fine segments, so round corners before them.
2. One batch per idea: `run_commands` with all the steps, so the person undoes the idea in one step.
3. `look` at the page afterwards; corners larger than the sides are limited automatically, so check the result rather than computing it.

## Gotchas

- Ellipses and lines have no corners; `path.roundCorners` refuses them.
- Editing anchors keeps live corners only while the anchor count stays the same; adding or deleting a point clears them.
- `path.offset` with a negative offset larger than the shape leaves nothing and is refused.
- Brushes, scatter and pattern tiles take a symbol by name, so `symbol.create` the tile first.
- A gradient mesh goes on a closed shape or frame, not on text or groups; in PDF it exports as a vector shading (in CMYK for PDF/X), while SVG and PDF/A get small filled shapes.
- In the window the person can also drag the corner handles shown under Direct Selection (Alt-click cycles the kind); tell them so when they want to fine-tune by hand.

## Hands back

The ids of the changed or new shapes, and a `look` of the page.
