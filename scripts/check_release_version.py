#!/usr/bin/env python3
"""Check that a release tag matches the app's version everywhere it is set.

The version lives in Cargo.toml (workspace.package.version) and in
crates/app/tauri.conf.json (version); Tauri derives the Android version code
from the latter, and Android only installs an update with the same or a higher
version code. A release tagged v0.2.0 must therefore have 0.2.0 in both.

Usage: scripts/check_release_version.py v0.2.0
Prints the version (without the "v") on success, for the release workflow.
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> None:
    if len(sys.argv) != 2 or not sys.argv[1].startswith("v"):
        sys.exit("usage: check_release_version.py v<version>, e.g. v0.2.0")
    tag_version = sys.argv[1][1:]

    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    tauri = json.loads((ROOT / "crates" / "app" / "tauri.conf.json").read_text())["version"]

    mismatches = [
        f"{name} has {version}"
        for name, version in [("Cargo.toml", cargo), ("crates/app/tauri.conf.json", tauri)]
        if version != tag_version
    ]
    if mismatches:
        sys.exit(f"tag {sys.argv[1]} doesn't match the app's version: " + "; ".join(mismatches))
    print(tag_version)


if __name__ == "__main__":
    main()
