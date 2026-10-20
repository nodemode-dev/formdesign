---
name: intake-curator
description: "Prepares material and the brief for FormDesign intake so the Make pass gets everything right the first time: which files to pass, how to write the brief, field lines, image roles and captions."
---

# Intake curator

## Purpose

Turns what the person handed over into one `intake` call whose pack needs no repair, and hands the pack id back to the director.

## Preconditions

Absolute paths for the material. Folders are fine: intake walks them.

## Procedure

1. **Pass everything relevant, once.** Markdown, Word, RTF, text, HTML, CSV, XLSX, PDF, PNG, JPG, WebP, GIF and folders. Intake sorts articles by length (the longest becomes the main story), keeps images with their alt text and captions, and records unreadable files as warnings instead of failing.
2. **Brief in the person's words, then fields.** Start with what they said ("A warm 6-page feature for Northlight, cream paper, orange accent"). Then add one `Key: value` line per fact the layout needs. Recognised keys: Headline or Title, Subhead or Deck, Kicker, Tagline, CTA, Secondary CTA, URL, Email, Phone, Address, Venue or Location, Date, Time or Doors, Price, Offer, Legal or Small print, Footer, Name, Role, Byline or Author, Issue, Intro, Summary, Quote, Brand or Company, Contact, Details, Credit, Caption. Values win over anything found in the files.
3. **Say the kind and size when known.** Words that set the kind: magazine, newsletter, book, cookbook, report, white paper, case study, proposal, brochure, tri-fold, flyer, poster, postcard, menu, catalogue, business card, letterhead, invoice, resume, certificate, invitation, ticket, slides or pitch deck, infographic, one-pager, Instagram or social post, Instagram story, banner, leaderboard, print ad, landing page, blog, article page, portfolio, product page, event page, email newsletter, press release, documentation. Sizes: Letter, A4, A5, Tabloid, `1080x1350`, `728x90`, "square", "16:9".
4. **Colours.** Hex codes or colour words ("navy and coral"). The first mid-tone becomes the accent, the second the second accent, a very dark one the dark panel colour, and a very light one the paper colour when the brief mentions paper or background.
5. **Read the summary, fix the pack only if needed.** If the main article or title is wrong, re-run intake with a clearer brief (Headline: line) rather than patching later. Image roles come from file names (logo, portrait, product, hero) and transparency; rename or say "the logo is studio-mark.png" in the brief when it matters.

## Gate

The summary's title, main article, image count and fields match what the person gave; no warning names a file they care about.

## Hands back

The pack id and the ranked templates.

## Owned resources

Shared `references/brief.md` holds the parser's exact rules.
