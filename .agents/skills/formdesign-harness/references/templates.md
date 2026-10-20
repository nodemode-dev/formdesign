# FormDesign built-in templates

Generated from the template library by `./formdesign agents sync`; do not edit by hand. The person's own templates are listed live by `list_templates`.

Columns: id, size in the template's unit, pages, kinds the Make pass ranks it for, flows (running stories that add continuation pages), slots it fills.

## Contents

- Magazine (9)
- Newsletter (5)
- Book (6)
- Report (7)
- Print (10)
- Stationery (8)
- Ad (12)
- Web (9)
- Presentation (5)

## Magazine

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `magazine-back-page-column` | 612 x 792 pt | 1 | magazine | body | byline, email, kicker, note, portrait, role, subtitle, title |
| `magazine-contents-grid` | 612 x 792 pt | 1 | magazine | none | brand, caption, feature1.body, feature1.page, feature1.title, feature10.body, feature10.page, feature10.title, feature2.body, feature2.page, feature2.title, feature3.body, feature3.page, feature3.title (+24) |
| `magazine-cover-bold` | 612 x 792 pt | 1 | magazine | none | brand, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, hero, issue, kicker, note, subtitle, tagline, title |
| `magazine-department-news` | 612 x 792 pt | 1 | magazine | none | caption, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title, feature5.body, feature5.title, image1, image2, issue (+6) |
| `magazine-feature-spread` | 612 x 792 pt | 2 | magazine, book, report | body | byline, caption, hero, kicker, subtitle, title |
| `magazine-interview-spread` | 612 x 792 pt | 2 | magazine | none | byline, credit, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title, feature5.body, feature5.title, feature6.body, feature6.title (+10) |
| `magazine-longread-swiss` | 595.28 x 841.89 pt | 2 | magazine, report | body | byline, date, intro, issue, kicker, note, pullquote, subtitle, title |
| `magazine-photo-essay` | 595.28 x 841.89 pt | 4 | magazine, book | none | byline, caption1, caption2, caption3, caption4, caption5, image1, image2, image3, image4, image5, kicker, quote, quotecredit (+3) |
| `magazine-reviews-culture` | 595.28 x 841.89 pt | 1 | magazine | none | image1, issue, item1.body, item1.price, item1.title, item2.body, item2.price, item2.title, item3.body, item3.price, item3.title, item4.body, item4.price, item4.title (+13) |

## Newsletter

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `newsletter-community-bulletin` | 792 x 1224 pt | 1 | newsletter, flyer | body | address, brand, contact, email, feature1.body, feature1.title, feature10.body, feature10.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title (+20) |
| `newsletter-company-quarterly` | 612 x 792 pt | 2 | newsletter, report | body | brand, caption, contact, cta, date, email, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title (+31) |
| `newsletter-nonprofit-four-page` | 595.28 x 841.89 pt | 4 | newsletter, report | body | address, brand, caption, caption2, cta, feature1.body, feature1.title, feature10.body, feature10.title, feature11.body, feature11.title, feature12.title, feature2.body, feature2.title (+45) |
| `newsletter-school-term` | 595.28 x 841.89 pt | 2 | newsletter | body | brand, byline, contact, cta, date, feature1.body, feature1.title, feature10.body, feature10.title, feature11.body, feature11.title, feature12.body, feature12.title, feature13.body (+49) |
| `newsletter-studio-update` | 612 x 792 pt | 1 | newsletter, one-pager | none | brand, email, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title, image1, image2, image3, intro (+5) |

## Book

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `book-cookbook-spread` | 576 x 720 pt | 2 | book, menu | body | hero, intro, kicker, list, note, stat1, stat1.label, stat2, stat2.label, stat3, stat3.label, title |
| `book-nonfiction-chapter` | 504 x 720 pt | 3 | book, report | body | caption, feature1.body, feature1.title, image1, kicker, list, note, pullquote, quotecredit, section, subtitle, title |
| `book-novel-interior` | 432 x 648 pt | 3 | book | body | kicker, quote, quotecredit, title |
| `book-photo-monograph` | 720 x 576 pt | 4 | book, catalog | none | byline, caption1, caption2, caption3, date, hero, image1, image2, image3, image4, intro, kicker, title |
| `book-picture-book` | 612 x 612 pt | 4 | book | body | brand, byline, hero, image1, image2, logo, title |
| `book-poetry-chapbook` | 396 x 612 pt | 3 | book | body | brand, byline, date, section, subtitle, title, toc |

## Report

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `report-annual-review` | 595.28 x 841.89 pt | 2 | report | none | brand, feature1.title, hero, intro, issue, kicker, logo, name, portrait, quote, role, section, stat1, stat1.label (+9) |
| `report-business-proposal` | 612 x 792 pt | 3 | report, one-pager | none | brand, contact, date, details, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title, feature5.body, feature5.title (+33) |
| `report-case-study` | 595.28 x 841.89 pt | 2 | report, one-pager, brochure | none | brand, cta, details, email, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, hero, image1, issue, kicker (+11) |
| `report-infographic-tall` | 1080 x 2700 px | 1 | infographic, poster, report | none | brand, feature1.body, feature1.title, feature2.body, feature2.title, feature3.title, feature4.body, feature4.title, feature5.body, feature5.title, feature6.body, feature6.title, feature7.body, feature7.title (+24) |
| `report-one-pager-summary` | 612 x 792 pt | 1 | one-pager, report, flyer | none | brand, caption, contact, date, email, feature1.body, feature1.title, image1, kicker, list, logo, stat1, stat1.label, stat2 (+6) |
| `report-research-brief` | 595.28 x 841.89 pt | 2 | report, newsletter | body | brand, byline, caption, image1, issue, list, section, subtitle, summary, title |
| `report-white-paper` | 612 x 792 pt | 2 | report | body | brand, byline, date, kicker, logo, section, subtitle, summary, title |

## Print

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `print-brochure-bifold` | 595.28 x 841.89 pt | 4 | brochure | none | address, brand, cta, email, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, hero, image1, image2, image3 (+24) |
| `print-brochure-trifold` | 792 x 612 pt | 2 | brochure | none | address, brand, cta, details, email, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title, feature5.body (+36) |
| `print-catalog-product-page` | 612 x 792 pt | 1 | catalog, brochure | none | brand, footer, image1, image2, image3, image4, image5, image6, intro, issue, item1.body, item1.price, item1.title, item2.body (+16) |
| `print-flyer-local-business` | 612 x 792 pt | 1 | flyer, poster | none | address, brand, cta, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, hero, intro, kicker, logo, phone (+5) |
| `print-menu-cafe-board` | 1190.55 x 841.89 pt | 1 | menu, poster | none | brand, details, hero, item1.body, item1.price, item1.title, item10.body, item10.price, item10.title, item11.body, item11.price, item11.title, item12.body, item12.price (+43) |
| `print-menu-restaurant` | 595.28 x 841.89 pt | 1 | menu | none | brand, item1.body, item1.price, item1.title, item10.body, item10.price, item10.title, item11.body, item11.price, item11.title, item12.body, item12.price, item12.title, item2.body (+30) |
| `print-postcard` | 432 x 288 pt | 2 | postcard | none | address, caption, date, footer, hero, intro, kicker, title |
| `print-poster-event-type` | 1296 x 1728 pt | 1 | poster, flyer | none | cta, eventdate, eventtime, intro, issue, kicker, list, location, title, url |
| `print-poster-exhibition-swiss` | 420 x 594 mm | 1 | poster | none | brand, eventdate, eventtime, kicker, location, note, subtitle, title, url |
| `print-poster-gig` | 792 x 1224 pt | 1 | poster, flyer | none | cta, date, eventdate, eventtime, hero, kicker, location, note, subtitle, title, url |

## Stationery

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `stationery-business-card` | 3.5 x 2 in | 2 | card | none | brand, details, logo, name, role |
| `stationery-certificate` | 792 x 612 pt | 1 | certificate | none | brand, byline, date, details, logo, name, note, role, title |
| `stationery-event-ticket` | 5.5 x 2 in | 1 | ticket | none | eventdate, eventtime, kicker, location, logo, name, note, subtitle, title |
| `stationery-gift-voucher` | 210 x 99 mm | 1 | card | none | brand, date, kicker, legal, logo, note, stat1, stat1.label, subtitle, title |
| `stationery-invitation` | 5 x 7 in | 1 | invitation | none | contact, eventdate, eventtime, kicker, location, note, subtitle, title |
| `stationery-invoice` | 595.28 x 841.89 pt | 1 | invoice | none | address, brand, contact, date, duedate, footer, issue, item1.body, item1.price, item1.qty, item1.rate, item1.title, item2.body, item2.price (+26) |
| `stationery-letterhead` | 612 x 792 pt | 1 | letterhead | body | address, brand, contact, date, logo, phone, tagline, title, url |
| `stationery-resume` | 595.28 x 841.89 pt | 1 | resume | none | details, feature1.body, feature1.meta, feature1.title, feature2.body, feature2.meta, feature2.title, feature3.body, feature3.meta, feature3.title, feature4.body, feature4.meta, feature4.title, feature5.meta (+9) |

## Ad

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `ad-banner-leaderboard` | 728 x 90 px | 1 | ad-banner | none | cta, logo, subtitle, title |
| `ad-banner-medium-rectangle` | 300 x 250 px | 1 | ad-banner | none | cta, hero, logo, subtitle, title |
| `ad-banner-mobile` | 320 x 100 px | 1 | ad-banner | none | cta, logo, subtitle, title |
| `ad-banner-skyscraper` | 160 x 600 px | 1 | ad-banner | none | cta, hero, kicker, logo, subtitle, title |
| `ad-event-announcement` | 1080 x 1350 px | 1 | ad-social | none | cta, eventdate, eventtime, hero, kicker, location, subtitle, title |
| `ad-link-landscape` | 1200 x 627 px | 1 | ad-social | none | brand, cta, hero, kicker, logo, subtitle, title |
| `ad-print-magazine-page` | 612 x 792 pt | 1 | ad-print | none | brand, cta, hero, intro, kicker, legal, logo, title, url |
| `ad-sale-promo` | 1080 x 1080 px | 1 | ad-social | none | cta, kicker, legal, logo, stat1, stat1.label, subtitle, title |
| `ad-social-portrait-product` | 1080 x 1350 px | 1 | ad-social | none | brand, cta, item1.price, kicker, logo, note, product, subtitle, title, url |
| `ad-social-square-bold` | 1080 x 1080 px | 1 | ad-social | none | cta, hero, logo, subtitle, title |
| `ad-social-square-minimal` | 1080 x 1080 px | 1 | ad-social | none | cta, kicker, logo, subtitle, title |
| `ad-story-stacked` | 1080 x 1920 px | 1 | ad-story | none | brand, cta, hero, logo, note, subtitle, title, url |

## Web

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `web-article-longread` | 1440 x 3000 px | 1 | web-article | body | brand, byline, caption, credit, cta, date, hero, kicker, nav, pullquote, subtitle, summary, title |
| `web-blog-journal` | 1440 x 2700 px | 1 | web-blog | none | brand, byline, cta, footer, image1, image2, image3, image4, image5, image6, image7, item1.body, item1.date, item1.title (+25) |
| `web-docs-reference` | 1440 x 1480 px | 1 | web-docs | body | brand, feature1.body, feature1.title, footer, kicker, nav, note, subtitle, title, toc |
| `web-email-digest` | 600 x 1720 px | 1 | web-email, newsletter | none | address, brand, cta, cta2, date, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, hero, image1, image2 (+8) |
| `web-event-conference` | 1440 x 3380 px | 1 | web-event | none | address, brand, cta, cta2, eventdate, eventtime, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.body, feature4.title (+41) |
| `web-landing-studio` | 1440 x 2600 px | 1 | web-landing, web-product, web-event | none | brand, closing, cta, cta2, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, footer, hero, image1, image2 (+9) |
| `web-portfolio-grid` | 1440 x 3260 px | 1 | web-portfolio | none | address, brand, caption1, caption2, caption3, caption4, caption5, caption6, caption7, cta, email, footer, gallery1, gallery2 (+20) |
| `web-press-release` | 1440 x 2140 px | 1 | web-press | body | address, brand, caption, cta, date, email, footer, hero, kicker, name, nav, note, phone, quote (+11) |
| `web-product-detail` | 1440 x 2460 px | 1 | web-product | none | brand, contact, cta, cta2, feature1.body, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, footer, image1, image2, image3 (+16) |

## Presentation

| id | size | pages | kinds | flows | slots |
| --- | --- | --- | --- | --- | --- |
| `presentation-keynote-dark` | 1920 x 1080 px | 6 | slides | none | caption, contact, date, feature1.body, feature1.title, feature2.title, feature3.title, image1, kicker, list, name, note, portrait, quote (+11) |
| `presentation-lookbook` | 1920 x 1080 px | 6 | slides, catalog | none | address, brand, byline, caption1, caption2, caption3, contact, feature1.body, feature1.title, hero, image1, image2, image3, image4 (+18) |
| `presentation-pitch-deck` | 1920 x 1080 px | 6 | slides | none | caption, email, feature1.body, feature1.title, feature2.body, feature2.title, feature3.title, feature4.title, feature5.body, feature5.title, hero, image1, image4, image5 (+34) |
| `presentation-quarterly-review` | 1920 x 1080 px | 6 | slides, report | none | byline, caption, date, email, feature1.title, feature2.body, feature2.title, feature3.body, feature3.title, feature4.title, item1.body, item1.title, item2.body, item2.title (+24) |
| `presentation-workshop-steps` | 1920 x 1080 px | 6 | slides | none | date, email, feature1.title, feature2.title, feature3.body, feature3.title, feature4.title, item1.body, item1.time, item1.title, item10.body, item10.title, item11.body, item11.title (+34) |
