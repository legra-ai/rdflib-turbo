"""Validate the initial alpha version and exact revision without mutating source."""

import os
import re
import subprocess
import tomllib
from pathlib import Path

from commit_policy import validate

project = tomllib.loads(Path("pyproject.toml").read_text())["project"]
version = project["version"]
if not re.fullmatch(r"\d+\.\d+\.\d+(?:a\d+)?", version):
    raise ValueError(f"Unsupported release version: {version}")
if Path("Cargo.toml").exists():
    rust_version = tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]
    if rust_version.replace("-alpha.", "a") != version:
        raise ValueError("Rust and Python versions differ")
revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
for header in subprocess.check_output(["git", "log", "--format=%s"], text=True).splitlines():
    validate(header)
with open(os.environ["GITHUB_OUTPUT"], "a") as output:
    output.write(f"version={version}\nrevision={revision}\n")
