"""Start Epicycle's Android emulator: a Pixel 10 running Android 16.

Packaged by the flake as `nix run .#android-emulator`, which fills in SDK below
with an Android SDK that has the emulator and the phone image. The virtual
phone lives in ~/.local/share/epicycle-android-emulator and is created on first
use; delete that folder to start from a fresh phone. Arguments are passed to the emulator,
e.g. `-no-window` to run it headless.
"""

import os
import subprocess
import sys
from pathlib import Path

SDK = "@sdk@"
NAME = "epicycle-pixel-10"
DEVICE = "pixel_10"
IMAGE = "system-images;android-36;google_apis;x86_64"
# Use the computer's GPU, and let its keyboard type into the phone.
CONFIG = {
    "hw.gpu.enabled": "yes",
    "hw.gpu.mode": "auto",
    "hw.keyboard": "yes",
}


def main() -> None:
    home = Path.home() / ".local" / "share" / "epicycle-android-emulator"
    avd_home = home / "avd"
    avd_home.mkdir(parents=True, exist_ok=True)
    env = {
        **os.environ,
        "ANDROID_HOME": SDK,
        "ANDROID_SDK_ROOT": SDK,
        "ANDROID_USER_HOME": str(home),
        "ANDROID_AVD_HOME": str(avd_home),
    }

    if not (avd_home / f"{NAME}.ini").exists():
        print(f"Creating the virtual phone ({DEVICE}) in {avd_home}", file=sys.stderr)
        subprocess.run(
            [
                f"{SDK}/cmdline-tools/22.0/bin/avdmanager",
                "create", "avd",
                "--name", NAME,
                "--package", IMAGE,
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

    emulator = f"{SDK}/emulator/emulator"
    os.execve(emulator, [emulator, "-avd", NAME, *sys.argv[1:]], env)


if __name__ == "__main__":
    main()
