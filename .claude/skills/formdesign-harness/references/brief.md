# How a brief is read

The brief is plain text: the person's words plus any `Key: value` lines. The parser (backend `app/intake/brief.py`) extracts:

| Field | Rule |
| --- | --- |
| kinds | Ordered by where they first appear in the text. "magazine", "zine", "quarterly", "feature spread" give magazine; "newsletter", "bulletin" newsletter; "book", "novel", "cookbook", "poetry" book; "report", "white paper", "case study", "proposal" report; "brochure", "tri-fold" brochure; "flyer"; "poster"; "postcard", "mailer"; "menu"; "catalogue", "lookbook", "price list"; "business card"; "letterhead"; "invoice"; "resume", "CV"; "certificate", "award"; "invitation", "wedding"; "ticket", "badge"; "slides", "pitch deck", "presentation"; "infographic"; "one-pager", "fact sheet"; "instagram", "social post", "square ad" ad-social; "instagram story", "reel", "9:16" ad-story; "banner", "leaderboard", "skyscraper", "300x250" ad-banner; "print ad", "magazine ad" ad-print; "landing page", "website", "homepage" web-landing; "blog" web-blog; "article page" web-article; "portfolio" web-portfolio; "product page" web-product; "event page" web-event; "email newsletter" web-email; "press release" web-press; "documentation" web-docs. |
| medium | web when a web kind is present, screen for social, banner and slides, else print. |
| tone | bold, minimal, elegant, playful, corporate, vintage, tech, warm, dark, from words such as punchy, clean, luxury, fun, professional, retro, startup, cozy, moody. |
| colours | `#rrggbb` codes and colour words (navy, coral, teal, cream, burgundy...). |
| size | `1080x1350`, `1080 x 1920 px`, `210x297 mm`; preset names Letter, Legal, Tabloid, A3 to A6; "square" 1080 x 1080, "instagram story" 1080 x 1920, "4:5", "16:9", "leaderboard", "skyscraper", "medium rectangle". |
| pages | "8-page", "12 pages". |
| cta | `CTA: ...`, a quoted button label, or phrases such as shop now, sign up, book now, register, subscribe, RSVP. |
| headline | `Headline:` or `Title:` lines. |
| brand | `Brand:` or `Company:` lines, or "for Kiln & Co, a ..." style phrases. |
| fields | every recognised `Key: value` line (see `intake-curator`), plus a detected email, URL, phone number, price and percentage offer. |

When the brief names no kind, the ranking infers one from the pack's title, file names and article titles with less weight, and otherwise prefers long-form templates for long text and short-form ones for short copy.
