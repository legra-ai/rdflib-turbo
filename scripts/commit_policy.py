"""Match the mandatory-scope contract of legra-ai/github-actions."""

import re
import sys
from pathlib import Path

HEADER = re.compile(
    r"^(feat|fix|build|chore|docs|style|ci|refactor|perf|test|revert)"
    r"\([a-z0-9][a-z0-9./-]*\)!?: \S(?:[^\r\n]*\S)?$"
)


def validate(message):
    header = message.splitlines()[0] if message else ""
    if not HEADER.fullmatch(header):
        raise ValueError(f"Scoped Conventional Commit required: {header!r}")


if __name__ == "__main__":
    validate(Path(sys.argv[1]).read_text())
