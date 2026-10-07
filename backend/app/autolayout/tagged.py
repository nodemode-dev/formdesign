import base64


def _esc(text: str) -> str:
    out = []
    for ch in text:
        if ch in "<>\\":
            out.append("\\" + ch)
        elif ch == "\t":
            out.append("\t")
        elif 32 <= ord(ch) < 127:
            out.append(ch)
        elif ord(ch) == 0x2028:
            out.append("<0x2028>")
        elif ord(ch) < 32:
            continue
        else:
            hx = ch.encode("utf-16-be").hex().upper()
            for i in range(0, len(hx), 4):
                out.append(f"<0x{hx[i:i + 4]}>")
    return "".join(out)


def paragraphs_to_tagged(paras: list[dict]) -> str:
    lines = ["<ASCII-MAC>"]
    for p in paras:
        style = p.get("style") or "Body"
        parts = [f"<ParaStyle:{style}>"]
        runs = p.get("runs")
        if runs:
            for r in runs:
                cs = r.get("cs")
                if cs:
                    parts.append(f"<CharStyle:{cs}>{_esc(r.get('text', ''))}<CharStyle:>")
                else:
                    parts.append(_esc(r.get("text", "")))
        else:
            parts.append(_esc(p.get("text", "")))
        lines.append("".join(parts))
    return "\n".join(lines)


def tagged_b64(paras: list[dict]) -> str:
    return base64.b64encode(paragraphs_to_tagged(paras).encode("ascii")).decode()
