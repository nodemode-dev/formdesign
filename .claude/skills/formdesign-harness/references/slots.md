# Slots

A slot is a frame labelled `slot:NAME`; a story that runs across frames and pages is labelled `flow:NAME` (usually `flow:body`). `get_slots` lists both for the active document; `set_slots` writes them by name; every frame with the same slot name receives the same value.

## What the Make pass fills

| Slot | From |
| --- | --- |
| `title` | brief Headline, else the main article's title, else the pack title |
| `subtitle` | the article's deck (front matter subtitle, or an italic first paragraph), else the pack subtitle |
| `kicker` | the article's section, category or first tag |
| `byline` | "By " plus the author |
| `date` | the article's date, else the brief's Date |
| `intro` | the first paragraph (removed from the running story) |
| `summary` | brief Summary, else the deck, else the first paragraph cut to about 60 words |
| `section`, `section2`... | the article's headings in order |
| `pullquote`, `quote` | strong sentences from the text (brief Quote wins), `quotecredit` the author |
| `caption`, `caption2`... | the caption or alt text of the image placed in `hero` or `imageN` |
| `stat1`, `stat1.label`... | numbers in the text with the words after them ("38%" and "rise in enrolment") |
| `feature1.title`, `feature1.body`... | secondary articles, then list items, then sections |
| `item1.title`, `item1.body`, `item1.price`... | table rows and list items ("Flat white: double shot, steamed milk, £3.40"; a price needs its currency sign or code) |
| `list` | the first bulleted list |
| `toc` | article titles, or the main article's headings |
| `cta`, `cta2`, `url`, `email`, `phone`, `address`, `location`, `eventdate`, `eventtime`, `price`, `offer`, `legal`, `footer`, `note`, `name`, `role`, `tagline`, `brand`, `contact`, `details`, `issue`, `credit`, `closing` | the brief's fields of the same meaning; `contact` and `details` join address, phone, email and URL; `footer` becomes a copyright line when the brand is known |
| `hero`, `image1`..., `portrait`, `product`, `background`, `gallery1`... | pack images matched by aspect ratio, role and resolution |
| `logo` | the image whose role is logo (a small transparent image, or a file name with logo or icon), else the brand kit's logo |
| `flow:body` | the main article as styled paragraphs: headings, body, quotes, lists, code and tables |

A slot with no content keeps the template's sample text and is listed under `kept`; an image frame with no picture gets on-palette sample art and is listed under `placeholders`; a slot whose content exists but is empty (an image without a caption) is cleared and listed under `emptied`.

## set_slots values

- Text: a string, or a list of strings for several paragraphs. The frame keeps its paragraph style.
- Image: a pack image id (`img3`) or an absolute file path. Logos are fitted inside the frame; other images fill it, cropped around the image's focal point.
- `flow:body`: markdown. Headings, lists, quotes and code become the template's styles; the story reflows through the existing frames (continuation pages are added only by make_layout).
- The reply lists `set`, `missing` (with the names that exist) and `overset` (slots whose text no longer fits: shorten it).
