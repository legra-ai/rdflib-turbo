"""Published decision references identify exactly one architectural contract."""

from pathlib import Path


def test_decision_numbers_are_unique():
    decisions = Path(__file__).resolve().parents[1] / "docs" / "adr"
    numbers = [path.name.split("-", 1)[0] for path in decisions.glob("[0-9]*-*.md")]
    assert len(numbers) == len(set(numbers)), "Architecture decision numbers must be unique"
