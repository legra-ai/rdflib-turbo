"""Create an alpha tag only at the tested manifest revision; reject conflicts."""

import os
import re
import subprocess
import tomllib

version = os.environ["RELEASE_VERSION"]
revision = os.environ["RELEASE_REVISION"]
if not re.fullmatch(r"\d+\.\d+\.\d+(?:a\d+)?", version) or not re.fullmatch(
    r"[0-9a-f]{40}", revision
):
    raise ValueError("Invalid release version or revision")
manifest = tomllib.loads(
    subprocess.check_output(["git", "show", f"{revision}:pyproject.toml"], text=True)
)
if manifest["project"]["version"] != version:
    raise ValueError("Release version does not match tested manifest")
tag = f"v{version}"
exists = subprocess.run(
    ["git", "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"],
    capture_output=True,
    text=True,
)
if exists.returncode == 0:
    if exists.stdout.strip() != revision:
        raise ValueError("Release tag identifies a different revision")
elif exists.returncode == 1:
    subprocess.run(
        [
            "git",
            "-c",
            "user.name=github-actions[bot]",
            "-c",
            "user.email=41898282+github-actions[bot]@users.noreply.github.com",
            "-c",
            "tag.gpgSign=false",
            "tag",
            "-a",
            tag,
            revision,
            "-m",
            f"Release {tag}",
        ],
        check=True,
    )
else:
    raise RuntimeError(exists.stderr)
subprocess.run(["git", "push", "origin", f"refs/tags/{tag}"], check=True)
