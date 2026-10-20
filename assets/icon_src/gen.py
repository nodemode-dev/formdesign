import math, sys

def squircle(cx, cy, size, n=3.0, pts=90):
    e = 0.30*size; h = size/2
    corners = [(cx+h-e, cy-h+e, 0), (cx+h-e, cy+h-e, 90), (cx-h+e, cy+h-e, 180), (cx-h+e, cy-h+e, 270)]
    out=[]
    for (ox, oy, start) in corners:
        for i in range(pts+1):
            t = math.radians(start - 90 + 90*i/pts)
            c, s_ = math.cos(t), math.sin(t)
            out.append(f"{ox+e*math.copysign(abs(c)**(2/n),c):.2f},{oy+e*math.copysign(abs(s_)**(2/n),s_):.2f}")
    return "M"+" L".join(out)+" Z"

RRECT = "M230,0 H794 A230,230 0 0 1 1024,230 V794 A230,230 0 0 1 794,1024 H230 A230,230 0 0 1 0,794 V230 A230,230 0 0 1 230,0 Z"

def page(cx, W, T0, B0, side, k):
    # side -1 left, +1 right; top edge lifts toward the spine then dips into it
    o = cx + side*W
    f = lambda x: f"{x:.1f}"
    top = (f"M{f(o)},{f(T0)} C{f(cx+side*W*0.55)},{f(T0-18*k)} {f(cx+side*34*k)},{f(T0-16*k)} {f(cx)},{f(T0+14*k)}")
    bot = (f"L{f(cx)},{f(B0+10*k)} C{f(cx+side*34*k)},{f(B0-4*k)} {f(cx+side*W*0.55)},{f(B0-6*k)} {f(o)},{f(B0)} Z")
    return top+" "+bot

def svg(mode="mac", detail="full", P="", compact=False):
    T = 824 if mode=="mac" else 1024
    cx = cy = 512
    k = 1.0 if mode=="mac" else 1.08
    if mode!="mac" and detail=="simple": k = 1.0
    simple = detail=="simple"
    W = (306 if not simple else 336)*k
    H = (468 if not simple else 500)*k
    T0 = cy - H/2 + 4*k; B0 = cy + H/2 + 4*k
    tile = RRECT if compact else squircle(cx, cy, T)
    L = page(cx, W, T0, B0, -1, k); R = page(cx, W, T0, B0, 1, k)
    cover_off = 16*k
    g=[f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
<defs>
 <linearGradient id="{P}bg" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#24212B"/><stop offset="1" stop-color="#0B0A0E"/></linearGradient>
 <radialGradient id="{P}bloom" cx="0.5" cy="0.44" r="0.5"><stop offset="0" stop-color="#FF4F7A" stop-opacity="0.26"/><stop offset="0.25" stop-color="#C255D9" stop-opacity="0.15"/><stop offset="0.5" stop-color="#9A5CFF" stop-opacity="0.07"/><stop offset="0.75" stop-color="#9A5CFF" stop-opacity="0.02"/><stop offset="1" stop-color="#9A5CFF" stop-opacity="0"/></radialGradient>
 <linearGradient id="{P}rim" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.22"/><stop offset="0.3" stop-color="#fff" stop-opacity="0.04"/><stop offset="1" stop-color="#fff" stop-opacity="0.08"/></linearGradient>
 <linearGradient id="{P}paperL" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#FBFAF7"/><stop offset="1" stop-color="#E2DED5"/></linearGradient>
 <linearGradient id="{P}paperR" x1="1" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#FFFFFD"/><stop offset="1" stop-color="#E8E4DC"/></linearGradient>
 <linearGradient id="{P}paint" gradientUnits="userSpaceOnUse" x1="{cx-W:.0f}" y1="{T0+260*k:.0f}" x2="{cx+W:.0f}" y2="{T0:.0f}"><stop offset="0" stop-color="#3A7BFF"/><stop offset="0.32" stop-color="#9A5CFF"/><stop offset="0.64" stop-color="#FF4F7A"/><stop offset="1" stop-color="#FFAE2B"/></linearGradient>
 <linearGradient id="{P}gutter" gradientUnits="userSpaceOnUse" x1="{cx-80*k:.0f}" y1="0" x2="{cx+80*k:.0f}" y2="0"><stop offset="0" stop-color="#000" stop-opacity="0"/><stop offset="0.42" stop-color="#000" stop-opacity="0.10"/><stop offset="0.5" stop-color="#000" stop-opacity="0.34"/><stop offset="0.56" stop-color="#000" stop-opacity="0.08"/><stop offset="1" stop-color="#000" stop-opacity="0"/></linearGradient>
 <linearGradient id="{P}sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.22"/><stop offset="0.5" stop-color="#fff" stop-opacity="0"/></linearGradient>
 <filter id="{P}tileshadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#000" flood-opacity="0.5"/></filter>
 <filter id="{P}bookshadow" x="-30%" y="-30%" width="160%" height="170%"><feDropShadow dx="0" dy="{18*k:.1f}" stdDeviation="{22*k:.1f}" flood-color="#000" flood-opacity="0.6"/></filter>
 <clipPath id="{P}tclip"><path d="{tile}"/></clipPath>
 <clipPath id="{P}pclip"><path d="{L}"/><path d="{R}"/></clipPath>
</defs>''']
    if mode=="mac" and not compact: g.append(f'<path d="{tile}" fill="#000" filter="url(#{P}tileshadow)"/>')
    g.append(f'<path d="{tile}" fill="url(#{P}bg)"/>')
    g.append(f'<g clip-path="url(#{P}tclip)"><rect width="1024" height="1024" fill="url(#{P}bloom)"/></g>')
    g.append(f'<g transform="rotate(-5 {cx} {cy})">')
    # cover beneath the pages
    cL = page(cx, W+10*k, T0+cover_off, B0+cover_off, -1, k); cR = page(cx, W+10*k, T0+cover_off, B0+cover_off, 1, k)
    cover = f'<g fill="#3A3743" stroke="#3A3743" stroke-width="{10*k:.1f}" stroke-linejoin="round"><path d="{cL}"/><path d="{cR}"/></g>'
    if compact: g.append(f'<g transform="translate(0 {12*k:.1f})" opacity=".35"><g fill="#000" stroke="#000" stroke-width="{30*k:.1f}" stroke-linejoin="round"><path d="{cL}"/><path d="{cR}"/></g></g>')
    else: g.append(f'<g filter="url(#{P}bookshadow)">{cover}</g>')
    g.append(cover)
    g.append(f'<path d="{L}" fill="url(#{P}paperL)"/><path d="{R}" fill="url(#{P}paperR)"/>')
    # content, clipped to the pages
    m = 40*k
    hx0, hx1 = cx-W+m, cx+W-m
    hy0 = T0 + (38 if not simple else 40)*k
    hh = (200 if not simple else 250)*k
    c = [f'<g clip-path="url(#{P}pclip)">']
    c.append(f'<rect x="{hx0:.1f}" y="{hy0:.1f}" width="{hx1-hx0:.1f}" height="{hh:.1f}" rx="{14*k:.1f}" fill="url(#{P}paint)"/>')
    c.append(f'<rect x="{hx0:.1f}" y="{hy0:.1f}" width="{hx1-hx0:.1f}" height="{hh:.1f}" rx="{14*k:.1f}" fill="url(#{P}sheen)"/>')
    # headline on the hero, left page
    bh = (30 if not simple else 46)*k
    c.append(f'<rect x="{hx0+28*k:.1f}" y="{hy0+hh-bh-(58 if not simple else 40)*k:.1f}" width="{(190 if not simple else 230)*k:.1f}" height="{bh:.1f}" rx="{bh/2:.1f}" fill="#fff" fill-opacity="0.96"/>')
    if not simple:
        c.append(f'<rect x="{hx0+28*k:.1f}" y="{hy0+hh-46*k:.1f}" width="{120*k:.1f}" height="{16*k:.1f}" rx="{8*k:.1f}" fill="#fff" fill-opacity="0.65"/>')
        ty = hy0+hh+34*k; lh = 30*k; th = 14*k; ink="#8F8A80"
        # left page: one column + pull quote rule
        for i,w in enumerate([250,250,250,170]):
            c.append(f'<rect x="{hx0:.1f}" y="{ty+i*lh:.1f}" width="{w*k:.1f}" height="{th:.1f}" rx="{th/2:.1f}" fill="{ink}" fill-opacity="0.8"/>')
        # right page: two columns
        colw = (W-m-28*k-34*k)/2
        for col in range(2):
            x0 = cx+28*k+col*(colw+34*k)
            for i,w in enumerate([1,1,1,0.62] if col==0 else [1,1,0.8,1]):
                c.append(f'<rect x="{x0:.1f}" y="{ty+i*lh:.1f}" width="{colw*w:.1f}" height="{th:.1f}" rx="{th/2:.1f}" fill="{ink}" fill-opacity="0.8"/>')
    if simple:
        ty = hy0+hh+44*k; th=26*k
        for i,w in enumerate([0.8,0.55]):
            c.append(f'<rect x="{hx0:.1f}" y="{ty+i*52*k:.1f}" width="{(W-m-30*k)*w:.1f}" height="{th:.1f}" rx="{th/2:.1f}" fill="#8F8A80" fill-opacity="0.8"/>')
            c.append(f'<rect x="{cx+30*k:.1f}" y="{ty+i*52*k:.1f}" width="{(W-m-30*k)*(1.0 if i==0 else 0.7):.1f}" height="{th:.1f}" rx="{th/2:.1f}" fill="#8F8A80" fill-opacity="0.8"/>')
    c.append(f'<rect x="{cx-90*k:.1f}" y="0" width="{180*k:.1f}" height="1024" fill="url(#{P}gutter)"/>')
    c.append('</g>')
    g += c
    g.append('</g>')
    g.append(f'<path d="{tile}" fill="none" stroke="url(#{P}rim)" stroke-width="{3*(T/824):.1f}" clip-path="url(#{P}tclip)"/>')
    g.append('</svg>')
    return "\n".join(g)

if __name__=="__main__":
    open(sys.argv[3],"w").write(svg(sys.argv[1], sys.argv[2]))
