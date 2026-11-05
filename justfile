# Build the app (only what changed) and open it.
run: deps
    cargo tauri build --no-bundle
    ./target/release/epicycle

# Open the app in development mode, reloading on UI changes.
dev: deps
    cargo tauri dev

# Run the Rust and frontend tests.
test: deps
    cargo test --workspace
    pnpm --dir ui test

# Run formatting, lint and type checks, and verify the sqlx query metadata.
check: deps
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    python3 scripts/sqlx_prepare.py --check
    pnpm --dir ui check

# Regenerate the sqlx query metadata after changing a query or migration.
sqlx-prepare:
    python3 scripts/sqlx_prepare.py

# Install frontend dependencies; quick when they already match the lockfile.
deps:
    pnpm --dir ui install --frozen-lockfile

# Android commands run in the Android dev shell (nix develop .#android), so
# they work from the default shell too. APKs are for 64-bit ARM phones.

# Build the current code and open it on the emulator, starting one if needed.
# Arguments go to the emulator. Needs KVM.
android-run *args:
    nix develop .#android --command python3 scripts/android_run.py {{args}}

# Build, install and open on the running emulator or connected phone, keeping
# the app's data.
android-install:
    nix develop .#android --command python3 scripts/android_run.py --running

# Run JavaScript in the open app and print the result (debug builds only).
android-eval script:
    nix develop .#android --command python3 scripts/android_eval.py {{quote(script)}}

# Restart the running emulator or connected phone, e.g. to check that reminders
# survive it.
android-reboot:
    nix develop .#android --command adb reboot

# Start the Android emulator (a virtual Pixel 10 with Android 16), then use
# `just android-dev` in another terminal. Arguments go to the emulator. Needs KVM.
android-emulator *args:
    nix run .#android-emulator -- {{args}}

# Run on a connected Android phone (USB debugging on) or a running emulator.
android-dev:
    nix develop .#android --command cargo tauri android dev

# Build a debug APK, signed with this machine's Android debug key.
android-debug:
    nix develop .#android --command cargo tauri android build --debug --apk --target aarch64

# Build a release APK, signed if crates/app/gen/android/keystore.properties exists.
android-release:
    nix develop .#android --command cargo tauri android build --apk --target aarch64

# Create the release signing key, once (see scripts/android_keystore.py).
android-keystore:
    nix develop .#android --command python3 scripts/android_keystore.py
