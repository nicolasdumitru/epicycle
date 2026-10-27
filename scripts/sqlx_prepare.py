#!/usr/bin/env python3
"""Regenerate sqlx's offline query metadata in .sqlx/.

Run this after changing an SQL query or a migration, and commit the result.
It builds a scratch database from the migrations and has `cargo sqlx prepare`
check every query against it. With --check, it only verifies that .sqlx/ is
up to date (for CI).
"""

import argparse
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS = ROOT / "crates" / "app" / "migrations"
DATABASE = ROOT / "target" / "sqlx" / "prepare.sqlite3"


def run(*args: str, env: dict[str, str]) -> None:
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail if .sqlx/ is out of date")
    check = parser.parse_args().check

    DATABASE.parent.mkdir(parents=True, exist_ok=True)
    for suffix in ("", "-wal", "-shm"):
        Path(f"{DATABASE}{suffix}").unlink(missing_ok=True)

    env = {**os.environ, "DATABASE_URL": f"sqlite://{DATABASE}", "SQLX_OFFLINE": "false"}
    run("sqlx", "database", "create", env=env)
    run("sqlx", "migrate", "run", "--source", str(MIGRATIONS), env=env)
    prepare = ["cargo", "sqlx", "prepare", "--workspace"]
    if check:
        prepare.append("--check")
    run(*prepare, "--", "--all-targets", env=env)


if __name__ == "__main__":
    main()
