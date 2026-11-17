#!/usr/bin/env python3
"""Epicycle's development tools on Windows, where Nix doesn't run.

Installs the tools of the Nix dev shells, at the versions in toolchain.json
(written from the flake by scripts/toolchain.py), then runs a shell or a
command with them, like `nix develop`:

    python scripts\\windows_env.py install              # the default dev shell's tools
    python scripts\\windows_env.py install --android    # and the Android shell's
    python scripts\\windows_env.py install --emulator   # and the emulator with its phone image
    python scripts\\windows_env.py shell                # a PowerShell with them
    python scripts\\windows_env.py run -- just check    # one command with them

Each download is checked against the checksum in toolchain.json. Tools go in
%LOCALAPPDATA%\\epicycle-dev (or EPICYCLE_TOOLS), one folder per version;
installing again replaces what changed and removes what's no longer used.
Rust comes from rustup, which reads rust-toolchain.toml. Needs Python 3.11 or
newer, rustup and the Visual Studio C++ build tools: see the README.
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

ROOT = Path(__file__).resolve().parent.parent
TOOLCHAIN = json.loads((ROOT / "toolchain.json").read_text())
TOOLS = Path(os.environ.get("EPICYCLE_TOOLS") or Path(os.environ.get("LOCALAPPDATA", Path.home())) / "epicycle-dev")
SDK = TOOLS / "android-sdk"
# What this script installed in the SDK: each package's path and checksum.
SDK_STATE = SDK / ".epicycle-packages.json"
# Marks a finished install with the checksum of what was installed.
DONE = ".epicycle-installed"


class Failure(Exception):
    """Something the person has to fix; the message says what."""


# Downloads


def download(url: str, algorithm: str, expected: str, directory: Path) -> Path:
    """Downloads into directory, failing unless the checksum matches."""
    path = directory / url.rsplit("/", 1)[1]
    digest = hashlib.new(algorithm)
    print(f"Downloading {url}", flush=True)
    with urllib.request.urlopen(url) as response, path.open("wb") as file:
        while chunk := response.read(1 << 20):
            digest.update(chunk)
            file.write(chunk)
    if digest.hexdigest() != expected:
        raise Failure(f"{url} doesn't match toolchain.json ({algorithm} {digest.hexdigest()}, expected {expected}).")
    return path


def extract(archive: Path, destination: Path, subfolder: str | None = None) -> None:
    """Unpacks into destination, replacing it.

    Takes the contents of subfolder, or else of the archive's single top-level
    folder if it has one (like nixpkgs does for Android packages).
    """
    staging = destination.with_name(destination.name + ".partial")
    shutil.rmtree(staging, ignore_errors=True)
    with zipfile.ZipFile(archive) as zip_file:
        zip_file.extractall(staging)
    root = staging / subfolder if subfolder else staging
    entries = list(root.iterdir())
    if subfolder is None and len(entries) == 1 and entries[0].is_dir():
        root = entries[0]
    shutil.rmtree(destination, ignore_errors=True)
    destination.parent.mkdir(parents=True, exist_ok=True)
    root.rename(destination)
    shutil.rmtree(staging, ignore_errors=True)


def tool_dir(name: str) -> Path:
    return TOOLS / f"{name}-{TOOLCHAIN['downloads'][name]['version']}"


def install_download(name: str, temp: Path, subfolder: str | None = None) -> None:
    """One tool from its publisher, unless that exact file is already installed."""
    spec = TOOLCHAIN["downloads"][name]
    destination = tool_dir(name)
    if (destination / DONE).exists() and (destination / DONE).read_text() == spec["sha256"]:
        return
    archive = download(spec["url"], "sha256", spec["sha256"], temp)
    extract(archive, destination, subfolder)
    (destination / DONE).write_text(spec["sha256"])
    archive.unlink()


def remove_old_versions(keep: set[Path]) -> None:
    """Removes other versions of the tools in TOOLS, e.g. after an update."""
    names = {path.name.rsplit("-", 1)[0] for path in keep}
    for path in TOOLS.iterdir():
        if path.is_dir() and path not in keep and path.name.rsplit("-", 1)[0] in names:
            print(f"Removing {path}", flush=True)
            shutil.rmtree(path)


# Android SDK


def xml_name(key: str) -> str:
    return key.split(":")[0]


def xml_order(key: str) -> int:
    """Google's element order, kept in the keys of nixpkgs' repo.json ("major:0")."""
    _, _, index = key.partition(":")
    return int(index) if index.isdigit() else 0


def xml_element(key: str, value: dict | str) -> str:
    tag = xml_name(key)
    if isinstance(value, str):
        return f"<{tag}>{escape(value)}</{tag}>"
    attributes = "".join(f" {name}={quoteattr(v)}" for name, v in value.get("element-attributes", {}).items())
    children = sorted((k for k in value if k != "element-attributes"), key=xml_order)
    if not children:
        return f"<{tag}{attributes}/>"
    return f"<{tag}{attributes}>{''.join(xml_element(k, value[k]) for k in children)}</{tag}>"


def package_xml(package: dict) -> str:
    """The package.xml sdkmanager would write, as nixpkgs' androidenv writes it.

    Android's tools (Gradle, avdmanager) read it to know what's installed.
    """
    license_name = package["license"]
    license_text = "---".join(TOOLCHAIN["android"]["licenses"][license_name]["texts"])
    dependencies = xml_element("dependencies", package["dependencies"]) if "dependencies" in package else ""
    namespaces = {
        "ns2": "http://schemas.android.com/repository/android/common/02",
        "ns3": "http://schemas.android.com/repository/android/common/01",
        "ns4": "http://schemas.android.com/repository/android/generic/01",
        "ns5": "http://schemas.android.com/repository/android/generic/02",
        "ns6": "http://schemas.android.com/sdk/android/repo/addon2/01",
        "ns7": "http://schemas.android.com/sdk/android/repo/addon2/02",
        "ns8": "http://schemas.android.com/sdk/android/repo/addon2/03",
        "ns9": "http://schemas.android.com/sdk/android/repo/repository2/01",
        "ns10": "http://schemas.android.com/sdk/android/repo/repository2/02",
        "ns11": "http://schemas.android.com/sdk/android/repo/repository2/03",
        "ns12": "http://schemas.android.com/sdk/android/repo/sys-img2/03",
        "ns13": "http://schemas.android.com/sdk/android/repo/sys-img2/02",
        "ns14": "http://schemas.android.com/sdk/android/repo/sys-img2/01",
        "xsi": "http://www.w3.org/2001/XMLSchema-instance",
    }
    declarations = "".join(f' xmlns:{prefix}="{uri}"' for prefix, uri in namespaces.items())
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f"<ns2:repository{declarations}>"
        f'<license id={quoteattr(license_name)} type="text">{escape(license_text)}</license>'
        f"<localPackage path={quoteattr(package['path'].replace('/', ';'))} obsolete={quoteattr(package.get('obsolete', 'false'))}>"
        f"{xml_element('type-details', package['type-details'])}"
        f"{xml_element('revision', package['revision-details'])}"
        f"{dependencies}"
        f"<display-name>{escape(package['displayName'])}</display-name>"
        f"<uses-license ref={quoteattr(license_name)}/>"
        "</localPackage></ns2:repository>\n"
    )


def patch_package(package: dict, directory: Path) -> None:
    """nixpkgs' fix for Google APIs phone images, which avdmanager otherwise
    doesn't recognise (see its androidenv/compose-android-packages.nix)."""
    if package["path"].startswith("system-images/") and "/google_apis" in package["path"]:
        properties = directory / "source.properties"
        lines = properties.read_text().splitlines(keepends=True)
        properties.write_text("".join(line for line in lines if not line.startswith("Addon.Vendor")))


def install_sdk(packages: list[dict], temp: Path) -> None:
    """The Android packages the flake composes, from Google's Windows downloads."""
    SDK.mkdir(parents=True, exist_ok=True)
    state = json.loads(SDK_STATE.read_text()) if SDK_STATE.exists() else {}
    for package in packages:
        archive_spec = package["archive"]
        directory = SDK / package["path"]
        if state.get(package["path"]) == archive_spec["sha1"] and directory.exists():
            continue
        archive = download(archive_spec["url"], "sha1", archive_spec["sha1"], temp)
        extract(archive, directory)
        archive.unlink()
        if not (directory / "package.xml").exists():
            (directory / "package.xml").write_text(package_xml(package), encoding="utf-8")
        patch_package(package, directory)
        state[package["path"]] = archive_spec["sha1"]
        SDK_STATE.write_text(json.dumps(state, indent=2))

    # Packages this script installed that the flake no longer has. The emulator
    # stays when installing without --emulator.
    android = TOOLCHAIN["android"]
    wanted = {p["path"] for p in android["packages"] + android["emulator"]["packages"]}
    for path in [p for p in state if p not in wanted]:
        print(f"Removing {SDK / path}", flush=True)
        shutil.rmtree(SDK / path, ignore_errors=True)
        del state[path]
    SDK_STATE.write_text(json.dumps(state, indent=2))

    # The flake accepts these licences (licenseAccepted); Gradle checks for them.
    (SDK / "licenses").mkdir(exist_ok=True)
    for name, license in android["licenses"].items():
        (SDK / "licenses" / name).write_text("\n".join(license["hashes"]))


# Rust, Node and Python packages


def run(*command: str | Path, env: dict[str, str] | None = None, cwd: Path = ROOT) -> None:
    environment = env or os.environ
    executable = shutil.which(str(command[0]), path=environment.get("PATH"))
    if executable is None:
        raise Failure(f"{command[0]} isn't installed or isn't on PATH.")
    subprocess.run([executable, *map(str, command[1:])], env=environment, cwd=cwd, check=True)


def install_rust(android: bool) -> None:
    """The flake's Rust version and components (rust-toolchain.toml), through rustup."""
    rust = TOOLCHAIN["rust"]
    components = [arg for c in rust["components"] for arg in ("--component", c)]
    run("rustup", "toolchain", "install", rust["version"], "--profile", rust["profile"], *components)
    if android:
        run("rustup", "target", "add", "--toolchain", rust["version"], *rust["androidTargets"])


def install_crates(env: dict[str, str]) -> None:
    for name, crate in TOOLCHAIN["crates"].items():
        options = ["--no-default-features"] if crate["noDefaultFeatures"] else []
        if crate["features"]:
            options += ["--features", ",".join(crate["features"])]
        run("cargo", "install", name, "--version", crate["version"], "--locked", "--root", TOOLS / "cargo", *options, env=env)


def install_pnpm(env: dict[str, str]) -> None:
    version = TOOLCHAIN["pnpm"]
    destination = TOOLS / f"pnpm-{version}"
    if (destination / DONE).exists():
        return
    # npm checks the package against the registry's integrity hash.
    run("npm", "install", "--global", "--prefix", destination, f"pnpm@{version}", env=env)
    (destination / DONE).write_text(version)


def install_python_packages(env: dict[str, str]) -> None:
    python = tool_dir("python") / "python.exe"
    if subprocess.run([python, "-m", "pip", "--version"], capture_output=True).returncode != 0:
        run(python, "-m", "ensurepip", env=env)
    packages = [f"{name}=={version}" for name, version in TOOLCHAIN["python"]["packages"].items()]
    run(python, "-m", "pip", "install", "--quiet", "--disable-pip-version-check", *packages, env=env)


# The environment


def ndk_dir() -> Path:
    return SDK / next(p["path"] for p in TOOLCHAIN["android"]["packages"] if p["name"] == "ndk")


def environment() -> dict[str, str]:
    """The current environment with the tools first on PATH, like the dev shells."""
    python = tool_dir("python")
    paths = [
        tool_dir("node"),
        TOOLS / f"pnpm-{TOOLCHAIN['pnpm']}",
        tool_dir("just"),
        python,
        python / "Scripts",
        TOOLS / "cargo" / "bin",
    ]
    env = dict(os.environ)
    if (SDK / "platform-tools").exists():
        paths += [
            tool_dir("jdk") / "bin",
            SDK / "platform-tools",
            SDK / "emulator",
            SDK / "cmdline-tools" / TOOLCHAIN["android"]["cmdlineTools"] / "bin",
        ]
        env |= {
            "JAVA_HOME": str(tool_dir("jdk")),
            "ANDROID_HOME": str(SDK),
            "ANDROID_SDK_ROOT": str(SDK),
            "NDK_HOME": str(ndk_dir()),
        }
    env["PATH"] = os.pathsep.join([*map(str, paths), env.get("PATH", "")])
    return env


def check_prerequisites() -> None:
    missing = []
    if shutil.which("rustup") is None:
        missing.append("rustup: winget install Rustlang.Rustup")
    vswhere = Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft Visual Studio/Installer/vswhere.exe"
    has_msvc = vswhere.exists() and subprocess.run(
        [vswhere, "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"],
        capture_output=True,
        text=True,
    ).stdout.strip()
    if not has_msvc:
        missing.append("the Visual Studio C++ build tools: see the README")
    if missing:
        raise Failure("Install these first:\n" + "\n".join(f"  - {m}" for m in missing))


def install(android: bool, emulator: bool) -> None:
    check_prerequisites()
    TOOLS.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=TOOLS) as temp_name:
        temp = Path(temp_name)
        install_download("node", temp)
        install_download("just", temp)
        # The NuGet package has Python itself in tools\.
        install_download("python", temp, subfolder="tools")
        keep = {tool_dir("node"), tool_dir("just"), tool_dir("python"), TOOLS / f"pnpm-{TOOLCHAIN['pnpm']}"}
        if android:
            install_download("jdk", temp)
            keep.add(tool_dir("jdk"))
            packages = TOOLCHAIN["android"]["packages"]
            if emulator:
                packages = packages + TOOLCHAIN["android"]["emulator"]["packages"]
            install_sdk(packages, temp)
    env = environment()
    install_pnpm(env)
    remove_old_versions(keep)
    install_python_packages(env)
    install_rust(android)
    install_crates(env)
    print(f"Done. Tools are in {TOOLS}; `python scripts\\windows_env.py shell` puts them on PATH.")


def shell() -> None:
    """PowerShell 7 if there is one, else Windows PowerShell."""
    program = shutil.which("pwsh") or shutil.which("powershell")
    if program is None:
        sys.exit("PowerShell not found.")
    print("A shell with Epicycle's tools; `exit` leaves it.", flush=True)
    sys.exit(subprocess.run([program, "-NoLogo"], env=environment(), cwd=ROOT).returncode)


def run_command(command: list[str]) -> None:
    env = environment()
    executable = shutil.which(command[0], path=env["PATH"])
    if executable is None:
        sys.exit(f"{command[0]}: not found. Have you run `python scripts\\windows_env.py install`?")
    sys.exit(subprocess.run([executable, *command[1:]], env=env).returncode)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    install_parser = commands.add_parser("install", help="install or update the tools")
    install_parser.add_argument("--android", action="store_true", help="also the Android SDK, NDK, JDK and Rust targets")
    install_parser.add_argument("--emulator", action="store_true", help="also the Android emulator and its phone image (implies --android)")
    commands.add_parser("shell", help="open a shell with the tools on PATH")
    run_parser = commands.add_parser("run", help="run one command with the tools on PATH")
    run_parser.add_argument("cmd", nargs=argparse.REMAINDER, help="the command, after --")
    options = parser.parse_args()

    try:
        if options.command == "install":
            install(options.android or options.emulator, options.emulator)
        elif options.command == "shell":
            shell()
        else:
            command = options.cmd[1:] if options.cmd[:1] == ["--"] else options.cmd
            if not command:
                parser.error("run needs a command, e.g. `run -- just check`")
            run_command(command)
    except Failure as failure:
        sys.exit(str(failure))
    except subprocess.CalledProcessError as error:
        sys.exit(f"Failed ({error.returncode}): {' '.join(map(str, error.cmd))}")


if __name__ == "__main__":
    main()
