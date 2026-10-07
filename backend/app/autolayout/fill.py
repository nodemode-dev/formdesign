from app.autolayout.tagged import tagged_b64

FOCAL_ALIGN = [(0, 0, 0), (1, 0, 1), (2, 0, 2), (0, 1, 3), (1, 1, 4), (2, 1, 5), (0, 2, 6), (1, 2, 7), (2, 2, 8)]


def focal_align(focal: list[float] | None) -> int:
    if not focal:
        return 4
    fx, fy = focal
    col = 0 if fx < 0.36 else 2 if fx > 0.64 else 1
    row = 0 if fy < 0.36 else 2 if fy > 0.64 else 1
    return row * 3 + col


def text_fill(frame_id: int, paras: list[dict], name: str = "slot") -> list[dict]:
    if not paras:
        paras = [{"style": None, "text": ""}]
    return [
        {"command": "selection.set", "params": {"ids": [frame_id]}},
        {"command": "file.place", "params": {"base64": tagged_b64(paras), "name": f"{name}.txt", "frame": frame_id, "styleConflicts": "useExisting"}},
    ]


def image_fill(frame_id: int, path: str, fit: str = "fill", focal: list[float] | None = None) -> list[dict]:
    mode = "fillProportionally" if fit == "fill" else "fitProportionally"
    return [
        {"command": "selection.set", "params": {"ids": [frame_id]}},
        {"command": "file.place", "params": {"path": path, "frame": frame_id}},
        {"command": "object.fittingOptions", "params": {"autoFit": True, "fitting": mode, "align": focal_align(focal), "ids": [frame_id]}},
        {"command": "object.fit", "params": {"mode": mode, "ids": [frame_id]}},
    ]
