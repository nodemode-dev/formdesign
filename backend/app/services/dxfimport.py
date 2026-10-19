import math
from collections import Counter
from pathlib import Path

import ezdxf
from ezdxf import colors as dxfcolors
from ezdxf import path as dxfpath
from ezdxf import recover, units
from ezdxf.math import BoundingBox2d, Vec2, Vec3

MARGIN = 36.0
SIDE = 800.0
MAX_OBJECTS = 20000
MAX_DEPTH = 8
DEFAULT_WEIGHT = 0.5
LINE_PITCH = 1.25
GLYPH_WIDTH = 0.62

PATHS = {"LINE", "ARC", "CIRCLE", "ELLIPSE", "SPLINE", "LWPOLYLINE", "POLYLINE", "SOLID", "TRACE"}
TEXTS = {"TEXT", "MTEXT", "ATTRIB"}
LABELS = {"LINE": "lines", "LWPOLYLINE": "polylines", "POLYLINE": "polylines", "ARC": "arcs", "CIRCLE": "circles", "ELLIPSE": "ellipses",
          "SPLINE": "splines (as polylines)", "SOLID": "solids (as outlines)", "TRACE": "solids (as outlines)", "HATCH": "solid hatches",
          "TEXT": "text frames", "MTEXT": "text frames", "ATTRIB": "text frames"}
MTEXT_ALIGN = {1: ("left", "top"), 2: ("center", "top"), 3: ("right", "top"), 4: ("left", "middle"), 5: ("center", "middle"),
               6: ("right", "middle"), 7: ("left", "bottom"), 8: ("center", "bottom"), 9: ("right", "bottom")}


class DxfError(Exception):
    pass


def read_doc(path: Path):
    try:
        return ezdxf.readfile(str(path))
    except ezdxf.DXFStructureError:
        pass
    except (OSError, UnicodeError) as exc:
        raise DxfError(f"{path.name} could not be read: {exc}") from exc
    try:
        doc, _ = recover.readfile(str(path))
        return doc
    except Exception as exc:
        raise DxfError(f"{path.name} is not a readable DXF file: {exc}") from exc


def aci_hex(aci: int) -> str:
    if aci == 7 or not 0 < aci < 256:
        return "#000000"
    c = dxfcolors.aci2rgb(aci)
    return "#%02x%02x%02x" % (c[0], c[1], c[2])


def rgb_hex(rgb) -> str:
    return "#%02x%02x%02x" % (rgb[0], rgb[1], rgb[2])


def weight_pt(hundredths) -> float | None:
    if hundredths is None or hundredths < 0:
        return None
    return max(0.25, hundredths / 100.0 * 72.0 / 25.4)


def flat_pts(sub, tol: float) -> list[tuple[float, float]]:
    out: list[tuple[float, float]] = []
    for v in sub.flattening(tol, segments=4):
        q = (float(v.x), float(v.y))
        if not out or abs(q[0] - out[-1][0]) > 1e-12 or abs(q[1] - out[-1][1]) > 1e-12:
            out.append(q)
    return out


def axis_rect(pts: list[tuple[float, float]], eps: float):
    if len(pts) != 5 or abs(pts[0][0] - pts[4][0]) > eps or abs(pts[0][1] - pts[4][1]) > eps:
        return None
    for k in range(4):
        a, b = pts[k], pts[k + 1]
        if abs(a[0] - b[0]) > eps and abs(a[1] - b[1]) > eps:
            return None
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs), min(ys), max(xs), max(ys)


class Reader:
    def __init__(self, doc) -> None:
        self.doc = doc
        self.shapes: list[dict] = []
        self.skipped: Counter = Counter()
        self.partial: Counter = Counter()
        self.hidden: Counter = Counter()
        self.layers: Counter = Counter()
        self.blocks = 0
        self._layer_info: dict[str, tuple[bool, str, float | None]] = {}

    def layer(self, name: str) -> tuple[bool, str, float | None]:
        if name not in self._layer_info:
            hidden, color, weight = False, "#000000", None
            if self.doc.layers.has_entry(name):
                ly = self.doc.layers.get(name)
                hidden = ly.is_off() or ly.is_frozen()
                rgb = ly.rgb
                color = rgb_hex(rgb) if rgb else aci_hex(ly.color)
                weight = weight_pt(ly.dxf.get("lineweight", -3))
            self._layer_info[name] = (hidden, color, weight)
        return self._layer_info[name]

    def style(self, e) -> tuple[str, float]:
        _, lcolor, lweight = self.layer(e.dxf.get("layer", "0"))
        if e.dxf.hasattr("true_color"):
            color = rgb_hex(dxfcolors.int2rgb(e.dxf.true_color))
        else:
            aci = e.dxf.get("color", 256)
            color = lcolor if aci in (0, 256) else aci_hex(aci)
        weight = weight_pt(e.dxf.get("lineweight", -1))
        if weight is None:
            weight = lweight if lweight is not None else DEFAULT_WEIGHT
        return color, weight

    def walk(self, entities, depth: int = 0) -> None:
        for e in entities:
            t = e.dxftype()
            name = e.dxf.get("layer", "0")
            if self.layer(name)[0] or e.dxf.get("invisible", 0):
                self.hidden[name] += 1
                continue
            if t == "INSERT":
                self.insert(e, depth)
            elif t == "DIMENSION":
                self.expand(e, depth, name)
            elif t in TEXTS:
                self.text(e, name)
            elif t == "HATCH":
                self.hatch(e, name)
            elif t == "POINT":
                self.skipped["POINT"] += 1
            elif t in PATHS:
                self.linear(e, name)
            else:
                self.skipped[t] += 1

    def note_skip(self, entity, reason: str) -> None:
        self.skipped[entity.dxftype()] += 1

    def expand(self, e, depth: int, name: str) -> list:
        if depth >= MAX_DEPTH:
            self.skipped[e.dxftype() + " (blocks nested too deeply)"] += 1
            return []
        try:
            subs = list(e.virtual_entities(skipped_entity_callback=self.note_skip))
        except Exception:
            self.skipped[e.dxftype()] += 1
            return []
        self.walk(subs, depth + 1)
        return subs

    def insert(self, e, depth: int) -> None:
        name = e.dxf.get("layer", "0")
        if not self.expand(e, depth, name) and not e.has_attrib:
            return
        self.blocks += 1
        if e.has_attrib:
            self.walk([a for a in e.attribs if a.dxf.get("text", "")], depth + 1)

    def linear(self, e, layer: str) -> None:
        t = e.dxftype()
        color, weight = self.style(e)
        flat = abs(e.dxf.get("extrusion", Vec3(0, 0, 1)).z) > 0.9999 if e.dxf.hasattr("extrusion") else True
        try:
            if t == "CIRCLE" and flat:
                c = e.ocs().to_wcs(e.dxf.center)
                r = float(e.dxf.radius)
                self.add({"kind": "ellipse", "label": t, "cx": c.x, "cy": c.y, "rx": r, "ry": r, "stroke": color, "weight": weight}, layer)
                return
            if t == "ELLIPSE" and flat and abs(e.dxf.end_param - e.dxf.start_param - math.tau) < 1e-6:
                m = e.dxf.major_axis
                size = math.hypot(m.x, m.y)
                if size > 0 and (abs(m.y) < size * 1e-6 or abs(m.x) < size * 1e-6):
                    along_x = abs(m.y) < size * 1e-6
                    rx, ry = (size, size * e.dxf.ratio) if along_x else (size * e.dxf.ratio, size)
                    c = e.dxf.center
                    self.add({"kind": "ellipse", "label": t, "cx": c.x, "cy": c.y, "rx": rx, "ry": ry, "stroke": color, "weight": weight}, layer)
                    return
            p = dxfpath.make_path(e)
        except Exception:
            self.skipped[t] += 1
            return
        if t in ("SOLID", "TRACE"):
            self.partial["solid fills drawn as outlines only"] += 1
        self.add({"kind": "path", "label": t, "path": p, "stroke": color, "weight": weight, "fill": None}, layer)

    def hatch(self, e, layer: str) -> None:
        if not e.dxf.get("solid_fill", 0):
            self.skipped["HATCH (pattern fills)"] += 1
            return
        if e.has_gradient_data:
            self.partial["gradient hatches filled with their first colour"] += 1
        color, _ = self.style(e)
        try:
            paths = list(dxfpath.from_hatch(e))
        except Exception:
            self.skipped["HATCH"] += 1
            return
        for p in paths:
            self.shapes.append({"kind": "path", "label": "HATCH", "path": p, "stroke": None, "weight": DEFAULT_WEIGHT, "fill": color})
        self.layers[layer] += 1

    def font_of(self, e) -> str:
        try:
            st = self.doc.styles.get(e.dxf.get("style", "Standard"))
            return Path(st.dxf.get("font", "") or "").stem
        except Exception:
            return ""

    def text(self, e, layer: str) -> None:
        t = e.dxftype()
        color, _ = self.style(e)
        try:
            body = e.plain_text()
            if t == "MTEXT":
                h = float(e.dxf.get("char_height", 0) or 0)
                at = Vec3(e.dxf.insert)
                halign, valign = MTEXT_ALIGN.get(e.dxf.get("attachment_point", 1), ("left", "top"))
                wrap = float(e.dxf.get("width", 0) or 0)
                rot = e.get_rotation()
            else:
                h = float(e.dxf.get("height", 0) or 0)
                hv, vv = e.dxf.get("halign", 0), e.dxf.get("valign", 0)
                src = e.dxf.align_point if (hv or vv) and e.dxf.hasattr("align_point") else e.dxf.insert
                at = e.ocs().to_wcs(src)
                halign = {1: "center", 2: "right", 4: "center"}.get(hv, "left")
                valign = "middle" if hv == 4 else {1: "bottom", 2: "middle", 3: "top"}.get(vv, "baseline")
                wrap = 0.0
                rot = float(e.dxf.get("rotation", 0) or 0)
        except Exception:
            self.skipped[t] += 1
            return
        lines = [ln.rstrip() for ln in body.replace("\r", "").split("\n")]
        while lines and not lines[-1]:
            lines.pop()
        if not any(lines):
            self.skipped[t + " (empty)"] += 1
            return
        if h <= 0:
            self.skipped[t + " (no height)"] += 1
            return
        if abs(math.remainder(rot, 360.0)) > 1.0:
            self.partial["rotated text frames placed horizontally"] += 1
        total = len(lines) * LINE_PITCH * h
        top = {"top": at.y, "middle": at.y + total / 2 if t == "MTEXT" else at.y + h / 2, "bottom": at.y + total if t == "MTEXT" else at.y + h,
               "baseline": at.y + h}[valign]
        width = wrap if wrap > 0 else GLYPH_WIDTH * h * max(len(ln) for ln in lines) + h
        x0 = at.x if halign == "left" else at.x - width / 2 if halign == "center" else at.x - width
        self.add({"kind": "text", "label": t, "x0": x0, "x1": x0 + width, "top": top, "bottom": top - total - 0.25 * h, "h": h, "lines": lines,
                  "align": halign, "color": color, "font": self.font_of(e), "stroke": None}, layer)

    def add(self, shape: dict, layer: str) -> None:
        self.shapes.append(shape)
        self.layers[layer] += 1


def rough_points(shape: dict) -> list[Vec2]:
    k = shape["kind"]
    if k == "ellipse":
        return [Vec2(shape["cx"] - shape["rx"], shape["cy"] - shape["ry"]), Vec2(shape["cx"] + shape["rx"], shape["cy"] + shape["ry"])]
    if k == "text":
        return [Vec2(shape["x0"], shape["bottom"]), Vec2(shape["x1"], shape["top"])]
    return [Vec2(v.x, v.y) for v in shape["path"].flattening(1e12, segments=8)]


def build(src: Path, sheet, options: dict | None = None) -> None:
    options = options or {}
    side = float(options.get("pageSide", SIDE))
    cap = int(options.get("maxObjects", MAX_OBJECTS))
    doc = read_doc(src)
    rd = Reader(doc)
    rd.walk(doc.modelspace())
    box = BoundingBox2d()
    for s in rd.shapes:
        box.extend(rough_points(s))
    if not box.has_data:
        raise DxfError(f"{src.name} has no visible drawing in its model space (paper space layouts are not imported)")
    span = max(box.size.x, box.size.y)
    if span <= 0:
        raise DxfError(f"{src.name} has only a single point in its model space")
    scale = (side - 2 * MARGIN) / span
    pw = round(box.size.x * scale + 2 * MARGIN, 3)
    ph = round(box.size.y * scale + 2 * MARGIN, 3)
    page = sheet.page(pw, ph)
    minx, maxy = box.extmin.x, box.extmax.y
    tol = span / 4000.0
    eps = span * 1e-6

    def X(x: float) -> float:
        return MARGIN + (x - minx) * scale

    def Y(y: float) -> float:
        return MARGIN + (maxy - y) * scale

    items = page["items"]
    made = 0
    drawn: Counter = Counter()
    skipped = rd.skipped

    def put(item: dict) -> None:
        item["order"] = len(items)
        items.append(item)

    for s in rd.shapes:
        k, label = s["kind"], LABELS[s["label"]]
        if made >= cap:
            skipped[f"{s['label']} (beyond the limit of {cap} objects)"] += 1
            continue
        if k == "ellipse":
            put({"kind": "rect", "shape": "ellipse", "rect": [X(s["cx"] - s["rx"]), Y(s["cy"] + s["ry"]), X(s["cx"] + s["rx"]), Y(s["cy"] - s["ry"])],
                 "fill": None, "stroke": s["stroke"], "weight": s["weight"]})
            made += 1
            drawn[label] += 1
        elif k == "text":
            size = max(1.0, s["h"] * scale)
            paras = [{"runs": [{"text": ln, "font": s["font"], "bold": False, "italic": False, "flags": 0, "color": s["color"], "size": round(size, 2)}],
                      "before": 0.0, "indent": 0.0} for ln in s["lines"]]
            put({"kind": "text", "rect": [X(s["x0"]), Y(s["top"]), X(s["x1"]), Y(s["bottom"])], "paras": paras, "align": s["align"],
                 "leading": round(size * LINE_PITCH, 2) if len(paras) > 1 else None, "firstSize": size, "base0": None})
            made += 1
            drawn[label] += 1
        else:
            before = made
            for sub in s["path"].sub_paths():
                pts = flat_pts(sub, tol)
                if len(pts) < 2:
                    continue
                box_ = axis_rect(pts, eps)
                if box_ and (s["fill"] or s["stroke"]):
                    x0, y0, x1, y1 = box_
                    put({"kind": "rect", "shape": "rectangle", "rect": [X(x0), Y(y1), X(x1), Y(y0)], "fill": s["fill"], "stroke": s["stroke"],
                         "weight": s["weight"]})
                    made += 1
                    continue
                color = s["stroke"] or s["fill"]
                if s["fill"]:
                    rd.partial["hatch boundaries other than rectangles drawn as outlines"] += 1
                for a, b in zip(pts, pts[1:]):
                    ax, ay, bx, by = X(a[0]), Y(a[1]), X(b[0]), Y(b[1])
                    if math.hypot(bx - ax, by - ay) < 0.01:
                        continue
                    put({"kind": "line", "a": [ax, ay], "b": [bx, by], "weight": s["weight"], "stroke": color})
                    made += 1
            if made > before:
                drawn[label] += 1
    for label, n in drawn.items():
        sheet.count(label, n)
    if made:
        sheet.count("shapes created", made)
    if rd.blocks:
        sheet.count("block references expanded", rd.blocks)
    unit = units.decode(doc.header.get("$INSUNITS", 0))
    sheet.note(f"{src.name}: model space only, DXF {doc.acad_release}, drawing units {unit or 'not stated'}, scaled to fit one page at {scale:.5g} points per drawing unit")
    names = sorted(rd.layers)
    for n in names[:40]:
        sheet.note(f"layer {n}: {rd.layers[n]} entities")
    if len(names) > 40:
        sheet.note(f"{len(names) - 40} more layers hold drawn entities")
    for n, c in sorted(rd.hidden.items()):
        sheet.note(f"layer {n} is off, frozen or invisible: {c} entities skipped")
    for t, c in sorted(skipped.items()):
        sheet.note(f"{c} {t} {'ignored' if t == 'POINT' else 'skipped'}")
    for t, c in sorted(rd.partial.items()):
        sheet.note(f"{c} {t}")


async def import_dxf(path: str, options: dict | None = None) -> dict:
    from app.engine.core import CoreError
    from app.services import layoutimport as li
    src = Path(path)
    if not src.is_file():
        raise CoreError(f"no file at {src}")
    sheet = li.Sheet(src.stem)
    try:
        build(src, sheet, options)
    except DxfError as exc:
        raise CoreError(str(exc)) from exc
    overview = await li.emit(sheet)
    return {"overview": overview, "report": li.finish_report(sheet, "dxf", src.name)}
