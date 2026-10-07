import re

KIND_WORDS = [
    ("web-landing", r"landing page|homepage|home page|website|web site|site\b|microsite|one[- ]page site"),
    ("web-blog", r"\bblog\b|blog post|blog index"),
    ("web-article", r"web article|article page|long[- ]?read online"),
    ("web-portfolio", r"portfolio"),
    ("web-product", r"product page|shop page|store page"),
    ("web-event", r"event page|conference site|rsvp page"),
    ("web-email", r"email newsletter|e-?mail|edm\b|mailchimp"),
    ("web-press", r"press release"),
    ("web-docs", r"documentation|docs page|help center"),
    ("ad-story", r"\bstor(y|ies)\b|reel|tiktok|9:16|1080x1920|1080 x 1920"),
    ("ad-social", r"instagram|social post|social media|facebook ad|linkedin post|1080x1080|1080x1350|square ad|carousel"),
    ("ad-banner", r"banner|leaderboard|skyscraper|display ad|300x250|728x90|160x600|300x600|970x250|mpu"),
    ("ad-print", r"print ad|magazine ad|full[- ]page ad|half[- ]page ad|newspaper ad"),
    ("ad-social", r"\bad\b|advert|advertisement|promo"),
    ("magazine", r"magazine|zine\b|feature spread|editorial spread|cover story|quarterly|monthly issue"),
    ("newsletter", r"newsletter|bulletin"),
    ("book", r"\bbook\b|novel|chapter|ebook|e-book|manuscript|cookbook|recipe book|poetry"),
    ("report", r"annual report|report\b|whitepaper|white paper|case study|research paper|proposal|impact report"),
    ("brochure", r"brochure|tri-?fold|bi-?fold|leaflet|pamphlet"),
    ("flyer", r"flyer|flier|handout"),
    ("poster", r"poster|gig poster|concert poster"),
    ("postcard", r"postcard|post card|mailer"),
    ("menu", r"\bmenu\b"),
    ("catalog", r"catalog|catalogue|lookbook|price list"),
    ("card", r"business card"),
    ("letterhead", r"letterhead|stationery"),
    ("invoice", r"invoice|receipt template|quote template"),
    ("resume", r"resume|résumé|\bcv\b|curriculum vitae"),
    ("certificate", r"certificate|diploma|award"),
    ("invitation", r"invitation|invite|wedding|save the date"),
    ("ticket", r"ticket|pass\b|badge"),
    ("slides", r"slides|slide deck|presentation|pitch deck"),
    ("infographic", r"infographic|data story|stats sheet"),
    ("one-pager", r"one[- ]pager|sell sheet|sales sheet|fact sheet|datasheet|data sheet"),
]

TONES = {
    "bold": r"bold|loud|punchy|striking|high[- ]contrast|impact",
    "minimal": r"minimal|clean|simple|swiss|whitespace|white space|understated|quiet",
    "elegant": r"elegant|luxury|luxurious|premium|refined|classy|sophisticated|editorial",
    "playful": r"playful|fun|friendly|whimsical|colorful|colourful|vibrant|kids",
    "corporate": r"corporate|professional|business|enterprise|formal|trustworthy",
    "vintage": r"vintage|retro|classic|nostalgic|old[- ]school|heritage",
    "tech": r"tech|techy|futuristic|digital|startup|saas|developer|ai\b",
    "warm": r"warm|cozy|organic|natural|earthy|handmade",
    "dark": r"dark mode|dark theme|moody|night|noir",
}

COLOR_NAMES = {
    "black": "#111111", "white": "#ffffff", "red": "#d62828", "orange": "#f77f00", "yellow": "#fcbf49", "gold": "#c9a227",
    "green": "#2a9d8f", "teal": "#0f766e", "blue": "#1d4ed8", "navy": "#14213d", "purple": "#6d28d9", "violet": "#7c3aed",
    "pink": "#ec4899", "magenta": "#c026d3", "brown": "#7f5539", "beige": "#e9dcc9", "cream": "#f5efe0", "gray": "#6b7280",
    "grey": "#6b7280", "mint": "#a7f3d0", "coral": "#ff6f61", "burgundy": "#7f1d1d", "olive": "#6b7d2a", "sand": "#e7d3b1",
}

PRESETS = {"a3": (842, 1191), "a4": (595.28, 841.89), "a5": (419.53, 595.28), "a6": (297.64, 419.53), "letter": (612, 792),
           "legal": (612, 1008), "tabloid": (792, 1224), "us letter": (612, 792)}


def parse(text: str | None, options: dict | None = None) -> dict:
    t = (text or "").strip()
    low = t.lower()
    options = options or {}
    kinds: list[str] = []
    for kind, pat in KIND_WORDS:
        if re.search(pat, low) and kind not in kinds:
            kinds.append(kind)
    if options.get("kind"):
        kinds.insert(0, options["kind"])
    tone = [k for k, pat in TONES.items() if re.search(pat, low)]
    colors = [c.lower() for c in re.findall(r"#[0-9a-fA-F]{6}\b", t)]
    for name, hx in COLOR_NAMES.items():
        if re.search(rf"\b{name}\b", low) and hx not in colors:
            colors.append(hx)
    size = None
    m = re.search(r"(\d{2,5})\s*[x×]\s*(\d{2,5})\s*(px|pt|mm|in)?", low)
    if m:
        size = {"w": float(m.group(1)), "h": float(m.group(2)), "unit": m.group(3) or "px"}
    else:
        for name, (w, h) in PRESETS.items():
            if re.search(rf"\b{name}\b", low):
                size = {"w": w, "h": h, "unit": "pt", "preset": name.upper() if len(name) == 2 else name.title()}
                break
    pages = None
    m = re.search(r"(\d{1,3})[- ]?(?:page|pp\b|pages)", low)
    if m:
        pages = int(m.group(1))
    cta = None
    m = re.search(r"(?:cta|call to action|button)\s*[:\-]?\s*[\"“']([^\"”']{2,40})[\"”']", t, re.I)
    if m:
        cta = m.group(1)
    else:
        m = re.search(r"\b(shop now|buy now|sign up|learn more|book now|get started|register|download|subscribe|order now|join us|rsvp)\b", low)
        if m:
            cta = m.group(1).title()
    headline = None
    m = re.search(r"(?:headline|title|tagline)\s*[:\-]\s*[\"“']?([^\"”'\n]{3,90})", t, re.I)
    if m:
        headline = m.group(1).strip()
    medium = "web" if any(k.startswith("web-") for k in kinds) else "screen" if any(k in ("ad-social", "ad-story", "ad-banner", "slides") for k in kinds) else "print"
    audience = None
    m = re.search(r"(?:audience(?: is)?|aimed at|targeting|written for|for readers who are)\s*:?\s+([a-z][a-z -]{2,40}?)(?:[.,;\n]|$)", low)
    if m:
        audience = m.group(1)
    brand = options.get("brand")
    if not brand:
        m = re.search(r"(?:brand|company|client|for)\s*[:\-]\s*([A-Z][\w&.' -]{1,40})", t)
        if m:
            brand = m.group(1).strip()
    return {"text": t, "kinds": kinds, "medium": options.get("medium") or medium, "tone": tone, "colors": colors[:6], "size": options.get("size") or size,
            "pages": options.get("pages") or pages, "cta": options.get("cta") or cta, "headline": options.get("headline") or headline,
            "audience": audience, "brand": brand}
