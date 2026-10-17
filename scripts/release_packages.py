"""Preserve tested PyPI archives and resume partial uploads by exact SHA-256."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import tomllib
import zipfile
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import urlopen


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def registry_json(url):
    try:
        with urlopen(url, timeout=60) as response:
            return json.load(response)
    except HTTPError as error:
        if error.code == 404:
            return None
        raise


def missing_packages(version, packages):
    metadata = registry_json(f"https://pypi.org/pypi/rdflib-turbo/{version}/json")
    published = (
        {entry["filename"]: entry["digests"]["sha256"] for entry in metadata["urls"]}
        if metadata
        else {}
    )
    if set(published) - {package.name for package in packages}:
        raise ValueError("PyPI contains unexpected release files")
    missing = []
    for package in packages:
        if package.name not in published:
            missing.append(package)
        elif published[package.name] != digest(package):
            raise ValueError(f"PyPI package identity differs from tested archive: {package.name}")
    return missing


def verify_packages(version, packages, *, attempts=31, interval=10):
    """Allow uploaded files to become visible; identity and service errors still fail."""
    missing = packages
    for attempt in range(attempts):
        missing = missing_packages(version, packages)
        if not missing:
            return
        if attempt + 1 < attempts:
            print(
                f"Waiting for PyPI to expose {len(missing)} uploaded archives "
                f"(query {attempt + 1}/{attempts})",
                flush=True,
            )
            time.sleep(interval)
    raise ValueError(f"PyPI release incomplete: {[path.name for path in missing]}")


def write_bundle(bundle, version, revision, packages):
    manifest = {
        "version": version,
        "revision": revision,
        "files": {package.name: digest(package) for package in packages},
    }
    if len(manifest["files"]) != len(packages):
        raise ValueError("Duplicate package filenames")
    with zipfile.ZipFile(bundle, "x", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, sort_keys=True))
        for package in packages:
            archive.write(package, package.name)


def read_bundle(bundle, version, revision, output):
    with zipfile.ZipFile(bundle) as archive:
        manifest = json.loads(archive.read("manifest.json"))
        if (manifest["version"], manifest["revision"]) != (version, revision):
            raise ValueError("Release bundle version/revision mismatch")
        names = archive.namelist()
        if len(names) != len(set(names)) or set(names) != {"manifest.json", *manifest["files"]}:
            raise ValueError("Release bundle contains unexpected files")
        output.mkdir()
        for name, expected in manifest["files"].items():
            if Path(name).name != name or name in (".", "..") or "\\" in name:
                raise ValueError("Invalid release package filename")
            target = output / name
            with archive.open(name) as source, target.open("xb") as destination:
                shutil.copyfileobj(source, destination)
            if digest(target) != expected:
                raise ValueError(f"Release bundle package identity mismatch: {name}")


def inventory(directory, version):
    packages = sorted(directory.iterdir())
    wheels = [path for path in packages if path.suffix == ".whl"]
    sdists = [path for path in packages if path.name == f"rdflib_turbo-{version}.tar.gz"]
    platforms = [
        "manylinux_2_28_x86_64",
        "manylinux_2_28_aarch64",
        "win_amd64",
    ]
    project = tomllib.loads(
        (Path(__file__).resolve().parents[1] / "pyproject.toml").read_text(encoding="utf-8")
    )
    targets = project["tool"]["maturin"]["target"]
    for triple, architecture in [
        ("x86_64-apple-darwin", "x86_64"),
        ("aarch64-apple-darwin", "arm64"),
    ]:
        minimum = targets[triple]["macos-deployment-target"].replace(".", "_")
        platforms.append(f"macosx_{minimum}_{architecture}")
    if len(packages) != 6 or len(sdists) != 1 or len(wheels) != 5:
        raise ValueError("Expected one source archive and five platform wheels")
    for platform in platforms:
        if not any(
            path.name == f"rdflib_turbo-{version}-cp311-abi3-{platform}.whl" for path in wheels
        ):
            raise ValueError(f"Missing platform wheel: {platform}")
    return packages


def stage(version, revision):
    tag = f"v{version}"
    bundle = Path("release-packages.zip")
    assets = json.loads(
        subprocess.check_output(["gh", "release", "view", tag, "--json", "assets"], text=True)
    )["assets"]
    if not any(asset["name"] == bundle.name for asset in assets):
        write_bundle(bundle, version, revision, inventory(Path("dist"), version))
        # No --clobber: retries must keep the original tested bytes.
        subprocess.run(["gh", "release", "upload", tag, str(bundle)], check=True)
    else:
        subprocess.run(["gh", "release", "download", tag, "--pattern", bundle.name], check=True)
    read_bundle(bundle, version, revision, Path("release-packages"))


def main():
    command = sys.argv[1]
    if command == "check-build":
        version = tomllib.loads(Path("pyproject.toml").read_text())["project"]["version"]
        inventory(Path("dist"), version)
        print("Verified one source archive and all five release wheel filenames")
        return
    version = os.environ["RELEASE_VERSION"]
    if command == "stage":
        stage(version, os.environ["RELEASE_REVISION"])
    elif command in ("filter", "verify"):
        packages = inventory(Path("release-packages"), version)
        if command == "verify":
            verify_packages(version, packages)
            return
        missing = missing_packages(version, packages)
        pending = Path("pending-packages")
        pending.mkdir()
        for package in missing:
            shutil.copyfile(package, pending / package.name)
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"missing={str(bool(missing)).lower()}\n")
    else:
        raise ValueError(f"Unknown release operation: {command}")


if __name__ == "__main__":
    main()
