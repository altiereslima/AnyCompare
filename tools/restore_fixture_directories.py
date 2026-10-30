"""Restore empty comparison fixtures, which Git does not retain as directories."""
from pathlib import Path

root = Path(__file__).resolve().parents[1]
manifest = root / "tests" / "fixtures" / "empty-directories.txt"
for relative in manifest.read_text(encoding="utf-8").splitlines():
    directory = (root / relative).resolve()
    if not directory.is_relative_to(root / "docs" / "reference"):
        raise ValueError(f"Fixture directory outside reference corpus: {relative}")
    directory.mkdir(parents=True, exist_ok=True)
