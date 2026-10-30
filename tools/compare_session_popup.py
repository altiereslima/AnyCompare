"""Finite popup geometry against preserved BC4 observations, not full pixel parity."""
import argparse
import json
from pathlib import Path
from PIL import Image


def popup_rect(path):
    image = Image.open(path).convert("RGB")
    pixels = image.load()
    for y in range(30, min(350, image.height)):
        x = 40
        while x < min(600, image.width):
            start = x
            while x < min(600, image.width) and all(abs(channel - 102) <= 1 for channel in pixels[x, y]):
                x += 1
            if x - start >= 100:
                width = x - start
                bottom = y + 1
                while bottom < image.height and all(abs(channel - 102) <= 1 for channel in pixels[start, bottom]):
                    bottom += 1
                if bottom - y >= 20:
                    return [start, y, width, bottom - y]
            x += 1
    raise ValueError(f"popup frame not found in {path}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("capture_root", type=Path)
    parser.add_argument("--reference", type=Path, default=Path(__file__).resolve().parents[1] / "docs/reference/sessions-runtime-2026-10-30/manifest.json")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    native = json.loads(args.reference.read_text(encoding="utf-8"))["observed"]
    checks = []
    english_width = None
    for language in ["pt-BR", "en"]:
        for state in ["collapsed", "expanded"]:
            path = args.capture_root / f"native-sessions-{state}-{language}.png"
            rect = popup_rect(path)
            expected = native[f"{state}_size"]
            if language == "en":
                # No original English runtime capture yet: only consistency and
                # row heights are checked, not English native width equivalence.
                if english_width is None:
                    english_width = rect[2]
                expected = [english_width, expected[1]]
            checks.append(dict(capture=path.name, rect=rect, expected_size=expected,
                               authority="native-runtime" if language == "pt-BR" else "internal-consistency",
                               passed=rect[2:] == expected))
    report = dict(scope="Popup body width/height only; no native theme, shadow or font/color equivalence", checks=checks)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if all(check["passed"] for check in checks) else 1


if __name__ == "__main__":
    raise SystemExit(main())
