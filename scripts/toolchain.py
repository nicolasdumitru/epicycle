#!/usr/bin/env python3
"""Write toolchain.json and rust-toolchain.toml from the Nix flake.

The dev shells' tools (flake.nix) are the only place versions are chosen.
Windows can't run Nix, so this writes them down for it: toolchain.json, which
scripts/windows_env.py installs from, and rust-toolchain.toml, which rustup
reads. Run it after `nix flake update` or after changing the tools in
flake.nix, and commit the result:

    just toolchain

The versions and Android packages come from `nix eval .#toolchain` (see
nix/toolchain.nix). This adds where each other tool's Windows build is
published, and its SHA-256, downloading only what changed since the last run.

With --check, it only verifies that both files are up to date, without
downloading anything (for CI).

`versions` prints the versions CI needs outside Nix, as KEY=VALUE lines for
$GITHUB_OUTPUT; it only reads toolchain.json, so it needs neither Nix nor the
network.
"""

import argparse
import hashlib
import json
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOLCHAIN = ROOT / "toolchain.json"
RUST_TOOLCHAIN = ROOT / "rust-toolchain.toml"
NOTE = "Generated from the Nix flake by scripts/toolchain.py: don't edit, run `just toolchain`."


def windows_downloads(tools: dict) -> dict[str, dict[str, str]]:
    """Where each tool's publisher keeps the 64-bit Windows build of that version.

    Every one of them keeps all past releases, so these stay valid.
    """
    node, just, python = tools["node"], tools["just"], tools["python"]["version"]
    # Adoptium names its Windows zip like the Linux tarball nixpkgs uses.
    jdk = tools["jdk"]["linuxUrl"]
    if "_x64_linux_" not in jdk or not jdk.endswith(".tar.gz"):
        sys.exit(f"Unexpected JDK download, can't find its Windows build: {jdk}")
    return {
        "node": {
            "version": node,
            "url": f"https://nodejs.org/dist/v{node}/node-v{node}-win-x64.zip",
        },
        "just": {
            "version": just,
            "url": f"https://github.com/casey/just/releases/download/{just}/just-{just}-x86_64-pc-windows-msvc.zip",
        },
        # python.org's own build, as a NuGet package: a plain zip with pip,
        # needing no installer.
        "python": {
            "version": python,
            "url": f"https://api.nuget.org/v3-flatcontainer/python/{python}/python.{python}.nupkg",
        },
        "jdk": {
            "version": tools["jdk"]["version"],
            "url": jdk.replace("_x64_linux_", "_x64_windows_").removesuffix(".tar.gz") + ".zip",
        },
    }


def sha256_of(url: str) -> str:
    print(f"Downloading {url}", file=sys.stderr, flush=True)
    digest = hashlib.sha256()
    with urllib.request.urlopen(url) as response:
        while chunk := response.read(1 << 20):
            digest.update(chunk)
    return digest.hexdigest()


def nix_toolchain() -> dict:
    result = subprocess.run(
        ["nix", "eval", "--json", ".#toolchain.x86_64-linux"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        sys.exit(result.stderr)
    return json.loads(result.stdout)


def rust_toolchain_toml(rust: dict) -> str:
    components = ", ".join(json.dumps(c) for c in rust["components"])
    return (
        f"# {NOTE}\n"
        "# rustup reads this outside Nix (Windows, the iOS build); the Nix dev\n"
        "# shells have their own Rust, of the same version.\n"
        "[toolchain]\n"
        f'channel = "{rust["version"]}"\n'
        f'profile = "{rust["profile"]}"\n'
        f"components = [{components}]\n"
    )


def as_json(data: dict) -> str:
    return json.dumps(data, indent=2, sort_keys=True) + "\n"


def generate(check: bool) -> None:
    tools = nix_toolchain()
    previous = json.loads(TOOLCHAIN.read_text()) if TOOLCHAIN.exists() else {}
    known = {d["url"]: d["sha256"] for d in previous.get("downloads", {}).values()}

    downloads = windows_downloads(tools)
    stale = []
    for name, download in downloads.items():
        if download["url"] in known:
            download["sha256"] = known[download["url"]]
        elif check:
            stale.append(name)
        else:
            download["sha256"] = sha256_of(download["url"])

    files = {
        TOOLCHAIN: as_json({"//": NOTE, **tools, "downloads": downloads}),
        RUST_TOOLCHAIN: rust_toolchain_toml(tools["rust"]),
    }
    if check:
        stale += [path.name for path, text in files.items() if not path.exists() or path.read_text() != text]
        if stale:
            sys.exit(f"Out of date ({', '.join(stale)}): run `just toolchain` and commit the result.")
        return
    for path, text in files.items():
        path.write_text(text)
        print(f"Wrote {path.relative_to(ROOT)}", file=sys.stderr)


def versions() -> None:
    tools = json.loads(TOOLCHAIN.read_text())
    print(f"rust={tools['rust']['version']}")
    print(f"node={tools['node']}")
    print(f"pnpm={tools['pnpm']}")
    print(f"tauri-cli={tools['crates']['tauri-cli']['version']}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", nargs="?", choices=["versions"], help="print the versions CI needs")
    parser.add_argument("--check", action="store_true", help="fail if the files are out of date")
    options = parser.parse_args()
    if options.command == "versions":
        versions()
    else:
        generate(options.check)


if __name__ == "__main__":
    main()
