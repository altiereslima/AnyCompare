"""Compare actual rendered editor edges and brackets with the user's BC4 capture.

100% DPI, 1616 x 838 client, resumo 1.txt on the left, resumo.txt on the right.
The original is scrolled to line 3; the candidate starts at line 1, hence 32 px
of vertical translation for the bracket masks only. No translation is allowed
for the panel edges. A channel tolerance of 1 covers the renderer's occasional
one-level shade variation in otherwise solid fills. Text glyphs are excluded.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'docs/reference/screenshots/text-loaded.png'


def near(pixel, value):
    return all(abs(channel - value) <= 1 for channel in pixel)


def vertical_edges(image, value, columns, bottom=730):
    return [x for x in columns
            if all(near(image.getpixel((x, y)), value) for y in range(130, bottom))]


def bracket_mask(image, gutter, value, start, end, shift=0):
    return {(x, y + shift) for x in range(gutter + 7, gutter + 15)
            for y in range(start, end) if near(image.getpixel((x, y)), value)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture', type=Path)
    parser.add_argument('--output', type=Path)
    opt = parser.parse_args()
    ours = Image.open(opt.capture).convert('RGB')
    original = Image.open(SOURCE).convert('RGB').crop((9, 42, 1625, 880))
    assert ours.size == original.size, (ours.size, original.size)
    checks = []

    def equal(name, expected, actual):
        checks.append(dict(name=name, expected=expected, actual=actual,
                           passed=expected == actual))

    def top_edges(image):
        return [y for y in range(109, 120)
                if all(near(image.getpixel((x, y)), 160)
                       for x in range(44, 810))]

    equal('panel_top_y', top_edges(original), top_edges(ours))
    equal('panel_left_edges_x',
          vertical_edges(original, 160, range(40, 1596)),
          vertical_edges(ours, 160, range(40, 1596)))
    columns = list(range(810, 832)) + list(range(1596, 1616))
    equal('viewport_and_panel_right_edges_x',
          vertical_edges(original, 255, columns),
          vertical_edges(ours, 255, columns))
    equal('viewport_edges_above_navigation_x',
          vertical_edges(original, 255, columns, bottom=694),
          vertical_edges(ours, 255, columns, bottom=694))
    for name, image in [('reference', original), ('candidate', ours)]:
        checks.append(dict(name=f'{name}_right_navigation_bevel',
                           passed=all(near(image.getpixel((1598, y)), 240)
                                      for y in range(694, 749))))
    # The three-pixel splitter lies between the left white frame and the
    # right dark frame. Verify every pixel, rather than just its midpoint.
    for name, image in [('reference', original), ('candidate', ours)]:
        checks.append(dict(name=f'{name}_splitter_3px',
                           passed=all(near(image.getpixel((x, y)), 240)
                                      for x in range(829, 832)
                                      for y in range(130, 730))))
    for side, gutter, value in [('active', 44, 105), ('inactive', 833, 132)]:
        expected = bracket_mask(original, gutter, value, 115, 717, shift=32)
        actual = bracket_mask(ours, gutter, value, 147, 749)
        checks.append(dict(name=f'{side}_bracket_mask',
                           reference_pixels=len(expected), candidate_pixels=len(actual),
                           missing_pixels=len(expected - actual),
                           extra_pixels=len(actual - expected),
                           passed=bool(expected) and expected == actual))
    report = dict(
        scope='Editor frames, 3px splitter, visible section brackets; not all UI pixels',
        source=str(SOURCE.relative_to(ROOT)),
        source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        capture=str(opt.capture),
        capture_sha256=hashlib.sha256(opt.capture.read_bytes()).hexdigest(),
        client_size=list(ours.size), color_tolerance=1, position_tolerance_px=0,
        bracket_scroll_translation_px=32, checks=checks,
    )
    output = json.dumps(report, ensure_ascii=False, indent=2) + '\n'
    if opt.output:
        opt.output.write_text(output, encoding='utf-8')
    print(output, end='')
    return 0 if all(check['passed'] for check in checks) else 1


if __name__ == '__main__':
    raise SystemExit(main())
