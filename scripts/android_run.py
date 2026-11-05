#!/usr/bin/env python3
"""Build the current code and open it on an emulator or phone, in one command.

Run in the Android dev shell, which `just android-run` enters by itself:

1. Finds a running emulator or connected phone. Without one, starts the
   emulator (`nix run .#android-emulator`) and leaves it running afterwards.
2. Builds a debug APK for that device's processor.
3. Installs it over the previous version, keeping the app's data, and opens it.

With --running it only uses a device that is already there, so
`just android-install` never starts an emulator. Any other arguments go to the
emulator, e.g. `just android-run -no-window`.
"""

import argparse
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = "io.github.nicolasdumitru.epicycle"
APK = ROOT / "crates/app/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk"
EMULATOR_LOG = Path.home() / ".local/share/epicycle-android-emulator/emulator.log"
# The first start may have to download the phone's system image.
BOOT_TIMEOUT_SECONDS = 15 * 60
# Android's processor names, and Tauri's for the targets the flake provides.
TARGETS = {"x86_64": "x86_64", "arm64-v8a": "aarch64"}


def adb(*args: str, serial: str | None = None) -> subprocess.CompletedProcess[str]:
    device = ["-s", serial] if serial else []
    return subprocess.run(["adb", *device, *args], capture_output=True, text=True)


def devices() -> dict[str, str]:
    """Connected devices and their states ("device", "offline", ...)."""
    lines = adb("devices").stdout.splitlines()[1:]
    return dict(line.split()[:2] for line in lines if line.strip())


def is_emulator(serial: str) -> bool:
    return serial.startswith("emulator-")


def choose(found: dict[str, str]) -> str | None:
    """The device to use: the only one, or else the only emulator.

    A booting emulator counts, even while adb calls it "offline": starting a
    second one on the same virtual phone would fail.
    """
    candidates = list(found)
    if len(candidates) > 1:
        candidates = [s for s in candidates if is_emulator(s)]
    if len(candidates) > 1:
        sys.exit(f"Several devices are connected ({', '.join(found)}); leave only one.")
    return candidates[0] if candidates else None


def start_emulator(args: list[str]) -> subprocess.Popen[bytes]:
    """Starts the emulator in its own session, so it outlives this script."""
    EMULATOR_LOG.parent.mkdir(parents=True, exist_ok=True)
    print(f"Starting the emulator (log: {EMULATOR_LOG})", flush=True)
    with EMULATOR_LOG.open("wb") as log:
        return subprocess.Popen(
            ["nix", "run", ".#android-emulator", "--", *args],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )


def booted(serial: str) -> bool:
    return adb("shell", "getprop", "sys.boot_completed", serial=serial).stdout.strip() == "1"


def wait_for_boot(emulator: subprocess.Popen[bytes] | None) -> str:
    print("Waiting for the device to start", flush=True)
    deadline = time.monotonic() + BOOT_TIMEOUT_SECONDS
    while time.monotonic() < deadline:
        if emulator is not None and emulator.poll() is not None:
            sys.exit(f"The emulator stopped; see {EMULATOR_LOG}")
        found = devices()
        serial = choose(found)
        if serial and found[serial] == "device" and booted(serial):
            return serial
        time.sleep(2)
    sys.exit(f"The device didn't start in time; see {EMULATOR_LOG}")


def target_of(serial: str) -> str:
    abi = adb("shell", "getprop", "ro.product.cpu.abi", serial=serial).stdout.strip()
    if abi not in TARGETS:
        sys.exit(f"No build for this device's processor ({abi or 'unknown'}).")
    return TARGETS[abi]


def build(target: str) -> None:
    subprocess.run(["pnpm", "--dir", "ui", "install", "--frozen-lockfile"], cwd=ROOT, check=True)
    subprocess.run(
        ["cargo", "tauri", "android", "build", "--debug", "--apk", "--target", target],
        cwd=ROOT,
        check=True,
    )


def install(serial: str) -> None:
    result = adb("install", "-r", str(APK), serial=serial)
    if result.returncode == 0:
        return
    output = result.stdout + result.stderr
    if "INSTALL_FAILED_UPDATE_INCOMPATIBLE" in output:
        # Uninstalling erases the app's data, so that stays the user's call.
        sys.exit(
            "Epicycle is already installed with a different signing key, probably a "
            "release build.\nUninstalling it erases its data. If that's fine:\n"
            f"  nix develop .#android --command adb -s {serial} uninstall {PACKAGE}"
        )
    sys.exit(f"Installing failed:\n{output}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--running", action="store_true", help="only use a device that is already there")
    options, emulator_args = parser.parse_known_args()

    emulator = None
    if choose(devices()) is None:
        if options.running:
            sys.exit("No device found: connect a phone, or start one with `just android-emulator`.")
        emulator = start_emulator(emulator_args)
    serial = wait_for_boot(emulator)

    build(target_of(serial))
    install(serial)
    adb("shell", "am", "start", "-n", f"{PACKAGE}/.MainActivity", serial=serial)
    print(f"Epicycle is open on {serial}.")


if __name__ == "__main__":
    main()
