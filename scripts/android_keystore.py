#!/usr/bin/env python3
"""Create the key that signs Android release builds, and set up signing.

Android only installs an update over an app signed with the same key, so this
key must be kept for as long as the app exists: back it up. Run it once, in the
Android dev shell (`just android-keystore`), which provides `keytool`:

1. Creates the keystore (default: ~/.android/epicycle-release.jks). keytool
   asks for the password itself, so it never appears in a command line.
2. Writes crates/app/gen/android/keystore.properties, which is git-ignored, so
   local release builds are signed too. It holds the password in plain text,
   like Android's usual setup; keep the file private.
3. Prints what to add as GitHub secrets for CI.

Nothing is sent anywhere.
"""

import argparse
import base64
import getpass
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PROPERTIES = ROOT / "crates" / "app" / "gen" / "android" / "keystore.properties"
ALIAS = "epicycle"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--keystore",
        type=Path,
        default=Path.home() / ".android" / "epicycle-release.jks",
        help="where to create the keystore (default: %(default)s)",
    )
    keystore = parser.parse_args().keystore.expanduser().resolve()

    if shutil.which("keytool") is None:
        sys.exit("keytool not found: run this in the Android dev shell (nix develop .#android).")
    if keystore.exists():
        sys.exit(f"{keystore} already exists; not overwriting a signing key.")

    keystore.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "keytool", "-genkeypair",
            "-keystore", str(keystore),
            "-storetype", "PKCS12",
            "-alias", ALIAS,
            "-keyalg", "RSA",
            "-keysize", "4096",
            # About 27 years; Android requires validity past 22 October 2033.
            "-validity", "10000",
            "-dname", "CN=Epicycle",
        ],
        check=True,
    )
    keystore.chmod(0o600)

    # PKCS12 keystores use one password for the store and the key.
    password = getpass.getpass("Enter the keystore password again, for keystore.properties: ")
    PROPERTIES.write_text(f"storeFile={keystore}\nkeyAlias={ALIAS}\npassword={password}\n")
    PROPERTIES.chmod(0o600)

    encoded = base64.b64encode(keystore.read_bytes()).decode()
    print(f"""
Created {keystore} and {PROPERTIES.relative_to(ROOT)}.

Back up the keystore and its password somewhere safe (e.g. a password manager):
without them, installed copies can't be updated, only uninstalled and
reinstalled, which deletes their data.

For CI, add these repository secrets on GitHub (Settings > Secrets and
variables > Actions > New repository secret):

  ANDROID_KEYSTORE_PASSWORD  the password you chose
  ANDROID_KEYSTORE_BASE64    the keystore, base64-encoded (printed below)

{encoded}
""")


if __name__ == "__main__":
    main()
