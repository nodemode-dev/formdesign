# Declined features

Features FormDesign will not build, with the reason, so no later session, agent or contributor adds them back. A request for one of these is answered by pointing here, not by building a variant of it.

| Feature | Decision | Reason |
| --- | --- | --- |
| Content-aware scale: resizing an image by removing or inserting low-energy pixel paths (seam carving, retargeting, liquid rescale) so chosen content keeps its proportions | Declined | Patent risk |
| Skin protection and protected regions for content-aware scale: detecting skin tones or a marked area so the resize avoids it | Declined | Patent risk |
| Shape Crafter choosing its operation (merge or erase) from where the stroke lands, rather than from the Alt modifier or `mode` | Declined | Patent risk |
| Liquid Layout rules that depend on a guide touching an object, or on a guide's orientation | Declined | Patent risk |
| Exporting Liquid Layout rules inside content for a viewer or reader to reflow when the page size changes | Declined | Patent risk |
| Trace Image between two clicked points: picking a start and an end point on a traced path, or choosing among candidate paths between them | Declined | Patent risk |

## What the rule covers

- No command, tool, menu item, panel control, MCP tool, CLI verb, plug-in, template step or agent recipe may implement, approximate or simulate either feature, under any name.
- No dependency that provides seam carving, retargeting or energy-map resizing may be added, in the core, the engine, the window or a plug-in.
- Ordinary image work is unaffected: scale, crop, fit, frame fitting, Edit Image, live adjustments and retouching stay as they are.
- Lifting a decline needs the owner's written decision after a patent review, recorded in this file first.
