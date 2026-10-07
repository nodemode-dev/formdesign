# Before saving a template

Go through every item against the validation result and the page images. An item that does not hold is fixed, not explained away.

1. `id` is kebab-case, starts with its category, and is not a built-in id; `name`, `description`, `kinds`, `tags` and `tone` are filled.
2. Validation: `ok` true, `problems` empty, `overset` empty, `preflight` has no errors.
3. Every content frame has a slot from the slot table or `flow`; decoration has none; repeated slots (brand in header and footer) are intended.
4. Rects derive from the margins, columns and gutter; edges align across frames on each page.
5. Sample text is original, the length real content will be, and free of em dashes, emoji, real brands and trademarks.
6. Colours use palette roles; the template still reads well with another brand's accent (try `/api/templates/{id}/thumb.png?brand=NAME` after saving, or validate a copy with a different theme).
7. Print templates: bleed set, edge items extended to the bleed, text inside the margins. Screen and web: `unit: "px"`, correct medium.
8. Running stories: the flow has a continuation page recipe and a sensible `max`.
9. Looked at every page image: clear hierarchy, nothing crowded or empty, images in sensible aspect ratios, text legible over images.
10. A test Make with a matching pack (an article, two or three photos, a logo, a brief) fills the slots it should and leaves no sample text in slots the pack covers.
