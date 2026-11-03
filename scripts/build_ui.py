#!/usr/bin/env python3
"""Build the frontend into ui/build, keeping the timestamps of unchanged files.

The release build embeds ui/build in the app, and Cargo recompiles the app crate
whenever one of those files is newer than the last build. SvelteKit rewrites
every file on each build, so without this every release build would recompile
and re-optimise the app, even when nothing changed.

Tauri runs this before each release build (see crates/app/tauri.conf.json).
"""

import hashlib
import os
import subprocess
import sys
from pathlib import Path

UI = Path(__file__).resolve().parent.parent / "ui"
BUILD = UI / "build"


def snapshot(root: Path) -> dict[Path, tuple[bytes, int]]:
    """Content hash and modification time of every file under `root`."""
    if not root.is_dir():
        return {}
    return {
        path.relative_to(root): (
            hashlib.sha256(path.read_bytes()).digest(),
            path.stat().st_mtime_ns,
        )
        for path in root.rglob("*")
        if path.is_file()
    }


def main() -> int:
    before = snapshot(BUILD)
    result = subprocess.run(["pnpm", "build"], cwd=UI)
    if result.returncode != 0:
        return result.returncode

    kept = 0
    for path, (digest, _) in snapshot(BUILD).items():
        old = before.get(path)
        if old is not None and old[0] == digest:
            full = BUILD / path
            os.utime(full, ns=(full.stat().st_atime_ns, old[1]))
            kept += 1
    print(f"build_ui: {kept} unchanged file(s) kept their timestamps")
    return 0


if __name__ == "__main__":
    sys.exit(main())
