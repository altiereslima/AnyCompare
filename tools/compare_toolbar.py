"""Compare a toolbar row of an AnyCompare capture with an original BC4 capture.

Finds separators, caption ink runs and icon columns in both images and prints the
column/row deltas. Usage:
  python -I tools/compare_toolbar.py <original.png> <orig_client_left> <orig_button_top>
                                      <ours.png> [<ours_button_top>]
The checked "Todos" button (fill 204,232,255) locates the button top when omitted.
"""
import sys
from PIL import Image


def load(path):
    im = Image.open(path).convert("RGB")
    return im, im.load()


def find_checked_top(px, w, h):
    for y in range(h):
        for x in range(w // 3):
            if px[x, y] == (153, 209, 255) and px[x, y + 1] == (204, 232, 255) if y + 1 < h else False:
                return y
    return None


def separators(px, w, top):
    out = []
    for x in range(w):
        dark = sum(1 for y in range(top + 3, top + 34) if sum(px[x, y]) < 3 * 170)
        if dark >= 28:
            out.append(x)
    return out


def caption_runs(px, w, top, x0=0):
    cols = [x for x in range(x0, w) if any(sum(px[x, y]) < 3 * 100 for y in range(top + 21, top + 34))]
    runs, s, p = [], None, None
    for x in cols:
        if s is None:
            s = p = x
        elif x - p > 5:
            runs.append((s, p))
            s = p = x
        else:
            p = x
    if s is not None:
        runs.append((s, p))
    return runs


def ink_rows(px, x0, x1, top):
    rows = []
    for y in range(top, top + 36):
        if any(sum(px[x, y]) < 3 * 100 for x in range(x0, x1)):
            rows.append(y - top)
    return rows


def main():
    orig, left, top = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    ours = sys.argv[4]
    oi, op = load(orig)
    ui, up = load(ours)
    utop = int(sys.argv[5]) if len(sys.argv) > 5 else find_checked_top(up, *ui.size)
    print("our button top:", utop)
    osep = [x - left for x in separators(op, oi.size[0] - 20, top)]
    usep = separators(up, ui.size[0] - 20, utop)
    print("separators orig:", osep)
    print("separators ours:", usep)
    oc = [(a - left, b - left) for a, b in caption_runs(op, oi.size[0] - 20, top, left)]
    uc = caption_runs(up, ui.size[0] - 20, utop)
    print("captions orig:", oc)
    print("captions ours:", uc)
    diffs = [(o, u) for o, u in zip(oc, uc) if o != u]
    print("caption runs differing:", len(diffs), diffs[:20])
    if oc:
        print("caption rows orig:", ink_rows(op, oc[0][0] + left, oc[0][1] + left, top)[-12:])
        print("caption rows ours:", ink_rows(up, uc[0][0], uc[0][1], utop)[-12:])


if __name__ == "__main__":
    main()
