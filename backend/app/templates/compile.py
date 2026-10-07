from app.autolayout.fill import text_fill

PAGE = ""


def risky(text: str) -> bool:
    body = text[1:]
    return "${" in text or (text.startswith("$") and bool(body) and "{" not in body and " " not in body)


def stroke_params(rt: dict, st: dict, ids: list) -> dict:
    w = float(st.get("weight", 1))
    out = {"swatch": swatch(rt, st.get("color", "ink")) or "Ink", "weight": w, "ids": ids}
    typ = st.get("type")
    if typ in ("dashed", "dash"):
        out["type"] = {"kind": "dashed", "pattern": [float(st.get("dash", w * 3)), float(st.get("gap", w * 2))]}
    elif typ == "dotted":
        out["type"] = {"kind": "dashed", "pattern": [0.01, float(st.get("gap", w * 2))]}
        out["cap"] = "round"
    elif typ:
        out["type"] = {"kind": typ}
    if st.get("cap"):
        out["cap"] = st["cap"]
    return out


def sides(rt: dict, n: int) -> list[str]:
    if not rt["facing"]:
        return ["single"] * n
    first = 0 if rt.get("startLeft") else 1
    return ["left" if (i + first) % 2 == 0 else "right" for i in range(n)]


def page_slots(rt: dict, n: int) -> list[tuple[int, float, str]]:
    out = []
    first = 0 if rt.get("startLeft") else 1
    for i, side in enumerate(sides(rt, n)):
        if rt["facing"]:
            spread = (i + first) // 2
            out.append((spread, rt["width"] if side == "right" and (i + first) % 2 == 1 and (i > 0 or rt.get("startLeft")) else 0.0, side))
        else:
            out.append((i, 0.0, side))
    return out


def parent_offset(rt: dict, side: str) -> float:
    return rt["width"] if rt["facing"] and side == "right" else 0.0


def margin_box(rt: dict, side: str) -> tuple[float, float, float, float]:
    m = rt["margins"]
    left, right = (m["outside"], m["inside"]) if side == "left" else (m["inside"], m["outside"])
    if side == "single":
        left, right = m["inside"], m["outside"]
    return left, m["top"], rt["width"] - left - right, rt["height"] - m["top"] - m["bottom"]


def rect_of(rt: dict, item: dict, side: str) -> tuple[float, float, float, float]:
    r = item.get("rect", "margins")
    W, H, b, u = rt["width"], rt["height"], rt["bleed"], rt["unit"]
    if r == "page":
        return 0.0, 0.0, W, H
    if r == "margins":
        return margin_box(rt, side)
    if r == "bleed":
        x0 = -b if side in ("single", "left", "right") and side != "right" else 0.0
        x1 = W + b if side in ("single", "right") else W
        if side == "right":
            x0 = 0.0
        return x0, -b, x1 - x0, H + 2 * b
    x, y, w, h = (float(v) * u for v in r)
    if item.get("mirror") and side == "left":
        x = W - x - w
    return x, y, w, h


def spread_rect(rt: dict, item: dict, side: str, offx: float) -> list[float]:
    x, y, w, h = rect_of(rt, item, side)
    return [round(x + offx, 3), round(y, 3), round(x + w + offx, 3), round(y + h, 3)]


def literal(text: str) -> str:
    return text.replace("<page>", PAGE).replace("<next>", "").replace("<prev>", "")


def text_paras(item: dict) -> list[str]:
    if item.get("text"):
        return literal(str(item["text"])).split("\n")
    s = item.get("sample")
    if item.get("flow") or not s:
        return [""]
    if isinstance(s, list):
        return [str(x) for x in s] or [""]
    return str(s).split("\n")


def swatch(rt: dict, role: str | None) -> str | None:
    if not role:
        return None
    if role in ("none", "[None]"):
        return "[None]"
    if role.startswith("#"):
        return None
    return rt["swatches"].get(role, role)


def is_print(rt: dict) -> bool:
    return rt.get("medium", "print") == "print"


def rgb_fractions(hx: str) -> dict:
    return {"model": "rgb", **{ch: round(int(hx[i:i + 2], 16) / 255, 5) for ch, i in (("r", 1), ("g", 3), ("b", 5))}}


def hexof(rt: dict, role: str) -> str:
    return role if role.startswith("#") else rt["palette"].get(role, "#000000")


class Builder:
    def __init__(self, rt: dict):
        self.rt = rt
        self.cmds: list[dict] = []
        self.slots: list[dict] = []
        self.items: list[dict] = []
        self.conv: dict[str, int] = {}
        self.flows: dict[str, list[int]] = {}

    def threads(self, results: list) -> list[dict]:
        out = []
        for chain in self.flows.values():
            ids = [results[i].get("id") for i in chain if i < len(results) and isinstance(results[i], dict)]
            out += [{"from": a, "to": c} for a, c in zip(ids, ids[1:]) if a is not None and c is not None]
        return out

    def stop_color(self, role: str):
        if not role.startswith("#") and role in self.conv:
            return f"${self.conv[role]}"
        return hexof(self.rt, role)

    def add(self, command: str, params: dict) -> int:
        self.cmds.append({"command": command, "params": params})
        return len(self.cmds) - 1

    def ref(self, i: int) -> str:
        return f"${i}.id"

    def style_object(self, i: int, item: dict) -> None:
        ids = [self.ref(i)]
        rt = self.rt
        if item.get("slot"):
            self.add("object.label", {"label": f"slot:{item['slot']}", "ids": ids})
        elif item.get("flow"):
            self.add("object.label", {"label": f"flow:{item['flow']}", "ids": ids})
        elif item.get("label"):
            self.add("object.label", {"label": item["label"], "ids": ids})
        if item.get("name"):
            self.add("object.rename", {"id": self.ref(i), "name": item["name"]})
        fill = item.get("fill")
        if isinstance(fill, str) and fill.startswith("#"):
            self.add("object.color", {"color": fill, "target": "fill", "ids": ids})
        elif fill:
            self.add("object.fill", {"swatch": swatch(rt, fill), "tint": float(item.get("tint", 1.0)), "ids": ids})
        g = item.get("gradient")
        if g:
            stops = [{"location": float(s[1]), "color": self.stop_color(s[0]), **({"opacity": float(s[2]) * 100} if len(s) > 2 else {})} for s in g.get("stops", [])]
            self.add("object.gradient", {"kind": g.get("kind", "linear"), "stops": stops, "angle": float(g.get("angle", 0)), "ids": ids})
        st = item.get("stroke")
        kind = item.get("kind")
        if st:
            self.add("object.stroke", stroke_params(rt, st, ids))
        elif kind in ("rect", "ellipse", "polygon"):
            self.add("object.stroke", {"swatch": "[None]", "ids": ids})
        if item.get("corner"):
            self.add("object.cornerOptions", {"shape": item.get("cornerShape", "rounded"), "size": float(item["corner"]) * rt["unit"], "ids": ids})
        if "opacity" in item:
            self.add("object.opacity", {"opacity": float(item["opacity"]), **({"blend": item["blend"]} if item.get("blend") else {}), "ids": ids})
        if item.get("wrap"):
            mode = {"box": "boundingBox", "contour": "contour", "jump": "jumpObject", "column": "jumpToNextColumn"}.get(item["wrap"], item["wrap"])
            self.add("object.textWrap", {"mode": mode, "offset": float(item.get("wrapOffset", 10)) * rt["unit"], "ids": ids})
        if item.get("shadow"):
            sh = item["shadow"] if isinstance(item["shadow"], dict) else {}
            self.add("object.dropShadow", {"on": True, "distance": float(sh.get("distance", 6)), "size": float(sh.get("size", 12)),
                                           "opacity": float(sh.get("opacity", 35)), "ids": ids})
        if item.get("rotate"):
            self.add("transform.rotate", {"angle": float(item["rotate"]), "ids": ids})
        if item.get("nonprinting"):
            self.add("object.attributes", {"nonprinting": True, "ids": ids})

    def text_options(self, i: int, item: dict) -> None:
        opts = {}
        if item.get("columns"):
            opts["columns"] = int(item["columns"])
            opts["gutter"] = float(item.get("gutter", self.rt["gutter"] / self.rt["unit"])) * self.rt["unit"]
        if "inset" in item:
            ins = item["inset"]
            opts["inset"] = [float(v) * self.rt["unit"] for v in ins] if isinstance(ins, list) else float(ins) * self.rt["unit"]
        if item.get("valign"):
            opts["verticalJustification"] = item["valign"]
        if item.get("ignoreWrap"):
            opts["ignoreWrap"] = True
        if opts:
            opts["ids"] = [self.ref(i)]
            self.add("object.textFrameOptions", opts)

    def item(self, item: dict, spread, side: str, offx: float, page: int | None) -> None:
        kind = item.get("kind", "text")
        rt = self.rt
        if kind == "line":
            u = rt["unit"]
            a = [float(item["from"][0]) * u, float(item["from"][1]) * u]
            b = [float(item["to"][0]) * u, float(item["to"][1]) * u]
            if item.get("mirror") and side == "left":
                a[0], b[0] = rt["width"] - a[0], rt["width"] - b[0]
            i = self.add("line.create", {"spread": spread, "a": [a[0] + offx, a[1]], "b": [b[0] + offx, b[1]]})
            self.style_object(i, {**item, "stroke": item.get("stroke") or {"color": "ink"}})
            return
        rect = spread_rect(rt, item, side, offx)
        if kind == "text":
            paras = text_paras(item)
            simple = len(paras) == 1 and not risky(paras[0])
            i = self.add("frame.create", {"spread": spread, "rect": rect, "content": "text", "text": paras[0] if simple else "",
                                          **({"shape": "ellipse"} if item.get("shape") == "ellipse" else {})})
            if item.get("style"):
                self.add("style.paragraph.apply", {"name": item["style"]})
            if not simple:
                for c in text_fill(self.ref(i), [{"style": item.get("style"), "text": t} for t in paras], item.get("slot") or "text"):
                    self.add(c["command"], c["params"])
            if item.get("flow"):
                self.flows.setdefault(item["flow"], []).append(i)
            self.text_options(i, item)
        elif kind == "image":
            i = self.add("frame.create", {"spread": spread, "rect": rect, "content": "graphic", **({"shape": "ellipse"} if item.get("shape") == "ellipse" else {})})
            self.add("object.fittingOptions", {"autoFit": True, "fitting": "fillProportionally" if item.get("fit", "fill") == "fill" else "fitProportionally",
                                               "align": 4, "ids": [self.ref(i)]})
        else:
            shape = {"rect": "rectangle", "ellipse": "ellipse", "polygon": "polygon"}.get(kind, "rectangle")
            i = self.add("frame.create", {"spread": spread, "rect": rect, "shape": shape, "content": "unassigned", **({"sides": int(item["sides"])} if item.get("sides") else {})})
        self.style_object(i, item)
        rec = {"cmd": i, "kind": kind, "page": page, "slot": item.get("slot"), "flow": item.get("flow"), "style": item.get("style"),
               "sample": item.get("sample"), "rect": rect, "spread": spread, "fit": item.get("fit", "fill"), "item": item}
        self.items.append(rec)
        if item.get("slot") or item.get("flow"):
            self.slots.append(rec)


def structure(rt: dict, title: str | None = None, extra_pages: int = 0) -> Builder:
    b = Builder(rt)
    pages = rt["pages"]
    n = len(pages) + extra_pages
    m = rt["margins"]
    b.add("file.new", {"width": round(rt["width"], 3), "height": round(rt["height"], 3), "pages": n, "facingPages": rt["facing"],
                       "margins": m, "columns": rt["columns"], "gutter": rt["gutter"], "bleed": rt["bleed"], "title": title or rt["name"]})
    if rt["facing"] and rt.get("startLeft"):
        b.add("layout.documentSetup", {"startPage": 2})
    if rt.get("medium") in ("web", "screen"):
        b.add("layout.documentSetup", {"intent": "web"})
    if rt["baseline"]:
        b.add("document.preferences", {"baselineGrid": {"start": m["top"], "increment": rt["baseline"]}})
    for role, name in rt["swatches"].items():
        hx = rt["palette"][role]
        if is_print(rt) and hx.startswith("#") and len(hx) == 7:
            b.conv[role] = b.add("color.convert", {"color": rgb_fractions(hx), "to": "cmyk"})
            b.add("swatch.create", {"name": name, "color": f"${b.conv[role]}"})
        else:
            b.add("swatch.create", {"name": name, "color": hx})
    for cs in rt["charStyles"]:
        b.add("style.character.create", cs)
    for ps in rt["paraStyles"]:
        b.add("style.paragraph.create", ps)
    letters = sorted(rt["parents"].keys())
    for k, letter in enumerate(letters):
        if k > 0 or letter != "A":
            b.add("layout.parents.new", {"prefix": letter, "name": "Parent"})
    paper = rt["palette"].get("paper", "#ffffff").lower()
    parent_index = {letter: (k if letters and letters[0] == "A" else k + 1) for k, letter in enumerate(letters)}
    for letter in letters:
        pidx = parent_index[letter]
        psides = ["left", "right"] if rt["facing"] else ["single"]
        spread = {"kind": "parent", "index": pidx}
        if paper not in ("#ffffff", "#fff"):
            for side in psides:
                b.item({"kind": "rect", "rect": "bleed", "fill": "paper"}, spread, side, parent_offset(rt, side), None)
        for item in rt["parents"][letter]:
            want = item.get("side", "both")
            for side in psides:
                if want in ("both", side) or side == "single":
                    b.item(item, spread, side, parent_offset(rt, side), None)
    if paper not in ("#ffffff", "#fff") and not letters:
        for i, (spread, offx, side) in enumerate(page_slots(rt, n)):
            b.item({"kind": "rect", "rect": "bleed", "fill": "paper"}, spread, side, offx, i)
    geo = page_slots(rt, n)
    for i, spec in enumerate(pages):
        parent = spec.get("parent", "A" if "A" in rt["parents"] else None)
        if parent != "A":
            b.add("layout.pages.applyParent", {"pages": [i], "parent": parent})
        spread, offx, side = geo[i]
        if paper not in ("#ffffff", "#fff") and letters and parent not in rt["parents"]:
            b.item({"kind": "rect", "rect": "bleed", "fill": "paper"}, spread, side, offx, i)
        for item in spec.get("items", []):
            b.item(item, spread, side, offx, i)
    b.add("edit.deselectAll", {})
    return b


def continuation(rt: dict, flow: str, page_index: int, spread: int, offx: float, side: str) -> Builder:
    b = Builder(rt)
    recipe = (rt["flow"].get(flow) or {}).get("page") or {"items": [{"kind": "text", "rect": "margins", "flow": flow, "columns": rt["columns"]}]}
    parent = recipe.get("parent", "A" if "A" in rt["parents"] else None)
    b.add("layout.pages.insert", {"count": 1, "after": page_index - 1, "parent": parent})
    paper = rt["palette"].get("paper", "#ffffff").lower()
    if paper not in ("#ffffff", "#fff") and parent not in rt["parents"]:
        b.item({"kind": "rect", "rect": "bleed", "fill": "paper"}, spread, side, offx, page_index)
    for item in recipe.get("items", []):
        b.item(item, spread, side, offx, page_index)
    return b
