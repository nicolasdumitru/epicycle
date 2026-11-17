"""Start Epicycle's Android emulator: a Pixel 10 running Android 16.

Packaged by the flake as `nix run .#android-emulator`, which fills in SDK,
IMAGE and CMDLINE_TOOLS below with an Android SDK that has the emulator and
the phone image. On Windows, run it directly (`just android-emulator`) in
`scripts/windows_env.py shell` after `install --emulator`: it then takes them
from ANDROID_HOME and toolchain.json, written from the same flake.

The virtual phone lives in ~/.local/share/epicycle-android-emulator and is
created on first use; delete that folder to start from a fresh phone.
Arguments are passed to the emulator, e.g. `-no-window` to run it headless.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

SDK = "@sdk@"
IMAGE = "@image@"
CMDLINE_TOOLS = "@cmdline_tools@"
NAME = "epicycle-pixel-10"
DEVICE = "pixel_10"
# Vulkan passthrough to the computer's GPU can hang: Android's WebView
# initialises Vulkan when it starts, and on some hosts the call never returns,
# which freezes the app on a white screen ("not responding"). Without it, the
# guest falls back to OpenGL, which still uses the GPU.
DEFAULT_FLAGS = ["-feature", "-Vulkan"]
# Use the computer's GPU, and let its keyboard type into the phone.
CONFIG = {
    "hw.gpu.enabled": "yes",
    "hw.gpu.mode": "auto",
    "hw.keyboard": "yes",
}
WINDOWS = sys.platform == "win32"


def settings() -> tuple[str, str, str]:
    """The SDK, the phone image and the command-line tools' version."""
    if not SDK.startswith("@"):
        return SDK, IMAGE, CMDLINE_TOOLS
    sdk = os.environ.get("ANDROID_HOME")
    if not sdk:
        sys.exit("ANDROID_HOME isn't set: run this in `python scripts/windows_env.py shell`.")
    toolchain = json.loads((Path(__file__).resolve().parent.parent / "toolchain.json").read_text())
    android = toolchain["android"]
    return sdk, android["emulator"]["image"], android["cmdlineTools"]


def main() -> None:
    sdk, image, cmdline_tools = settings()
    home = Path.home() / ".local" / "share" / "epicycle-android-emulator"
    avd_home = home / "avd"
    avd_home.mkdir(parents=True, exist_ok=True)
    env = {
        **os.environ,
        "ANDROID_HOME": sdk,
        "ANDROID_SDK_ROOT": sdk,
        "ANDROID_USER_HOME": str(home),
        "ANDROID_AVD_HOME": str(avd_home),
    }

    if not (avd_home / f"{NAME}.ini").exists():
        print(f"Creating the virtual phone ({DEVICE}) in {avd_home}", file=sys.stderr)
        avdmanager = Path(sdk, "cmdline-tools", cmdline_tools, "bin", "avdmanager.bat" if WINDOWS else "avdmanager")
        subprocess.run(
            [
                str(avdmanager),
                "create", "avd",
                "--name", NAME,
                "--package", image,
                "--device", DEVICE,
            ],
            # Answers "no" to "create a custom hardware profile?".
            input="no\n",
            text=True,
            env=env,
            check=True,
        )
        with (avd_home / f"{NAME}.avd" / "config.ini").open("a") as config:
            for key, value in CONFIG.items():
                config.write(f"{key}={value}\n")

    emulator = str(Path(sdk, "emulator", "emulator.exe" if WINDOWS else "emulator"))
    arguments = [emulator, "-avd", NAME, *DEFAULT_FLAGS, *sys.argv[1:]]
    if WINDOWS:
        # Windows has no exec: wait for the emulator and pass on how it ended.
        sys.exit(subprocess.run(arguments, env=env).returncode)
    os.execve(emulator, arguments, env)


if __name__ == "__main__":
    main()
