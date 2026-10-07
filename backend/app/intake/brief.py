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
    ("ad-story", r"\b(?:instagram|ig|facebook|snapchat|whatsapp)\s+stor(?:y|ies)\b|\bstor(?:y|ies) ad\b|\breels?\b|tiktok|9:16|1080x1920|1080 x 1920"),
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

NUMBER_WORDS = {"one": 1, "two": 2, "three": 3, "four": 4, "five": 5, "six": 6, "eight": 8, "ten": 10, "twelve": 12, "sixteen": 16, "twenty": 20,
                "twenty-four": 24, "thirty-two": 32}
WEEKDAY = r"(?:mon|tues|wednes|thurs|fri|satur|sun)day"
MONTH = r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sept?(?:ember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)"
DATE_RE = re.compile(rf"\b((?:{WEEKDAY},?\s+)?(?:\d{{1,2}}(?:st|nd|rd|th)?\s+{MONTH}\b\.?|{MONTH}\.?\s+\d{{1,2}}(?:st|nd|rd|th)?\b)(?:,?\s+\d{{4}})?|{WEEKDAY}\b)", re.I)
CLOCK = r"\d{1,2}(?:[:.]\d{2})?\s?(?:am|pm|a\.m\.|p\.m\.)"
TIME_RE = re.compile(rf"\b({CLOCK}(?:\s?(?:-|\N{{EN DASH}}|to|until|till)\s?{CLOCK})?|noon|midday)", re.I)
ORG_NOUNS = r"(?:club|society|association|council|school|church|library|gallery|museum|studio|studios|collective|foundation|trust|group|team|cafe|bakery|restaurant|bar|shop|store|press|company|co|inc|ltd|llc)"
EVENT_NOUNS = {"sale", "fair", "party", "launch", "festival", "fest", "concert", "gig", "show", "exhibition", "market", "night", "dinner", "lunch",
               "brunch", "workshop", "class", "meetup", "conference", "summit", "opening", "fundraiser", "tasting", "screening", "race", "run",
               "tournament", "auction", "reading", "talk", "webinar", "gala", "picnic", "parade", "jam", "showcase", "retreat", "camp",
               "cleanup", "clean-up", "open day", "tour", "premiere", "ceremony", "celebration", "quiz", "bazaar", "expo", "hackathon"}


FIELD_ALIASES = {
    "headline": "title", "title": "title", "subhead": "subtitle", "subheadline": "subtitle", "subtitle": "subtitle", "deck": "subtitle",
    "standfirst": "subtitle", "kicker": "kicker", "eyebrow": "kicker", "section": "kicker", "tagline": "tagline", "slogan": "tagline",
    "cta": "cta", "call to action": "cta", "button": "cta", "secondary cta": "cta2", "cta2": "cta2", "url": "url", "website": "url",
    "web": "url", "link": "url", "email": "email", "e-mail": "email", "phone": "phone", "tel": "phone", "telephone": "phone",
    "address": "address", "location": "location", "venue": "location", "where": "location", "date": "eventdate", "when": "eventdate",
    "event date": "eventdate", "time": "eventtime", "doors": "eventtime", "price": "price", "offer": "offer", "discount": "offer",
    "legal": "legal", "disclaimer": "legal", "small print": "legal", "fine print": "legal", "footer": "footer", "note": "note",
    "name": "name", "role": "role", "job title": "role", "position": "role", "byline": "byline", "author": "byline", "by": "byline",
    "issue": "issue", "intro": "intro", "summary": "summary", "quote": "pullquote", "pull quote": "pullquote", "brand": "brand",
    "company": "brand", "client": "brand", "contact": "contact", "details": "details", "credit": "credit", "caption": "caption",
}


def fields_from(text: str) -> dict:
    out: dict = {}
    segments = [s for line in text.splitlines() for s in re.split(r"(?<=[.!?])\s+(?=[A-Z][A-Za-z -]{0,20}:)", line)]
    for seg in segments:
        m = re.match(r"\s*[-*]?\s*([A-Za-z][A-Za-z0-9 -]{0,20}?)\s*:\s*(.+?)\s*$", seg)
        if not m:
            continue
        key = FIELD_ALIASES.get(m.group(1).strip().lower())
        val = m.group(2).strip().strip("\"“”")
        if val.endswith(".") and not val.endswith(".."):
            val = val[:-1]
        if key and val and key not in out:
            out[key] = val
    m = re.search(r"[\w.+-]+@[\w-]+\.[\w.-]+", text)
    if m and "email" not in out:
        out["email"] = m.group(0)
    m = re.search(r"\b(?:https?://)?(?:www\.)?[a-z0-9-]+\.(?:com|org|net|io|co|studio|shop|store|app|dev|design|art|uk|eu|ai)(?:/[\w/-]*)?\b", text, re.I)
    if m and "url" not in out and "@" not in text[max(0, m.start() - 1):m.start()]:
        out["url"] = m.group(0)
    m = re.search(r"(?:\+\d{1,3}[ .-]?)?\(?\d{3}\)?[ .-]\d{3}[ .-]\d{4}\b", text)
    if m and "phone" not in out:
        out["phone"] = m.group(0)
    m = re.search(r"(?<![\w.])([$£€]\s?\d[\d,]*(?:\.\d{2})?)", text)
    if m and "price" not in out:
        out["price"] = m.group(1)
    m = re.search(r"\b(\d{1,3}\s?%\s?off|save \d{1,3}\s?%)", text, re.I)
    if m and "offer" not in out:
        out["offer"] = m.group(1)
    m = DATE_RE.search(text)
    if m and "eventdate" not in out:
        out["eventdate"] = m.group(1)
    m = TIME_RE.search(text)
    if m and "eventtime" not in out:
        out["eventtime"] = m.group(1)
    m = re.search(rf"\b((?:spring|summer|autumn|fall|winter|holiday|{MONTH})(?:\s+\d{{4}})?)\s+(?:issue|edition)\b|\bissue\s+(?:no\.?\s*)?(\d{{1,4}})\b", text, re.I)
    if m and "issue" not in out:
        out["issue"] = m.group(1).title() if m.group(1) else f"Issue {m.group(2)}"
    return out


def parse(text: str | None, options: dict | None = None) -> dict:
    t = (text or "").strip()
    low = t.lower()
    options = options or {}
    found: dict[str, int] = {}
    for kind, pat in KIND_WORDS:
        m = re.search(pat, low)
        if m and kind not in found:
            found[kind] = m.start()
    kinds: list[str] = sorted(found, key=lambda k: found[k])
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
    if size is None:
        for word, dims in (("square", (1080, 1080)), ("instagram story", (1080, 1920)), ("story ad", (1080, 1920)), ("reel", (1080, 1920)),
                           ("portrait post", (1080, 1350)), ("4:5", (1080, 1350)), ("16:9", (1920, 1080)), ("leaderboard", (728, 90)),
                           ("skyscraper", (160, 600)), ("medium rectangle", (300, 250)), ("mpu", (300, 250))):
            if re.search(rf"\b{re.escape(word)}\b", low):
                size = {"w": dims[0], "h": dims[1], "unit": "px", "hint": word}
                break
    pages = None
    m = re.search(r"\b(\d{1,3}|" + "|".join(sorted(NUMBER_WORDS, key=len, reverse=True)) + r")[- ]?(?:page|pp\b|pages)", low)
    if m:
        pages = int(m.group(1)) if m.group(1).isdigit() else NUMBER_WORDS[m.group(1)]
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
        else:
            m = re.search(rf"\b(?:for|from|by|at)\s+(?:the\s+)?([A-Z][\w'.-]*(?:\s+(?:&|and|of|[A-Z][\w'.-]*)){{0,4}}(?:\s+(?:[a-z]+\s+)?{ORG_NOUNS})?)(?=,|\s+(?:a|an|the|is|which|who|on|in|this|next|about|with|to|that|and)\b|\.|$)", t)
            words = m.group(1).split() if m else []
            if words and words[0].lower() not in {"instagram", "facebook", "linkedin", "our", "my", "a"} and not (words[0] == "The" and len(words) == 1) \
                    and not re.fullmatch(WEEKDAY, words[0].lower()):
                brand = m.group(1).strip()
            m = re.search(r"\b([A-Z][\w'-]*(?:\s+[A-Z][\w'-]*){0,3})\s+(?:magazine|newsletter|journal|quarterly|review|gazette|weekly|zine)\b", t)
            if not brand and m and m.group(1).split()[0] not in {"The", "A", "An", "Our", "My", "This", "New"}:
                brand = m.group(1)
    subject = None
    if brand and brand.split()[-1].lower() in EVENT_NOUNS:
        subject, brand = brand, options.get("brand")
    if not subject:
        m = re.search(r"\b(?:for|about|announcing|promoting|advertising)\s+(?:a|an|our|the)\s+((?:[\w'&-]+\s+){0,3}?(" + "|".join(sorted(EVENT_NOUNS, key=len, reverse=True)) + r"))\b", low)
        if m:
            words = m.group(1).split()
            subject = " ".join([words[0].capitalize()] + words[1:])
    fields = {**fields_from(t), **(options.get("fields") or {})}
    return {"text": t, "kinds": kinds, "medium": options.get("medium") or medium, "tone": tone, "colors": colors[:6], "size": options.get("size") or size,
            "pages": options.get("pages") or pages, "cta": options.get("cta") or fields.get("cta") or cta,
            "headline": options.get("headline") or headline or fields.get("title"), "audience": audience, "brand": brand or fields.get("brand"),
            "subject": subject, "fields": fields}
