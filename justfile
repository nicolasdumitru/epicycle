# On Windows, run these in `python scripts\windows_env.py shell`, the
# equivalent of the Nix dev shell (see the README).
set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

python := if os_family() == "windows" { "python" } else { "python3" }
# Android commands enter the Android dev shell on Linux, so they work from the
# default one too. On Windows the tools are already there.
android_shell := if os_family() == "windows" { "" } else { "nix develop .#android --command" }

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
    {{python}} scripts/sqlx_prepare.py --check
    pnpm --dir ui check
    cargo doc --workspace --no-deps --document-private-items --config "build.rustdocflags=['-D','warnings']"

# Build the Rust API docs, private items included, into target/doc. Add
# `--open` to open them in a browser.
docs *args:
    cargo doc --workspace --no-deps --document-private-items {{args}}

# Regenerate the sqlx query metadata after changing a query or migration.
sqlx-prepare:
    {{python}} scripts/sqlx_prepare.py

# Install frontend dependencies; quick when they already match the lockfile.
deps:
    pnpm --dir ui install --frozen-lockfile

# Write the flake's tool versions to toolchain.json and rust-toolchain.toml,
# for Windows. Run after `nix flake update` or changing the tools in flake.nix.
[unix]
toolchain:
    python3 scripts/toolchain.py

# APKs are for 64-bit ARM phones.

# Build the current code and open it on the emulator, starting one if needed.
# Arguments go to the emulator. Needs KVM on Linux, the hypervisor on Windows.
android-run *args:
    {{android_shell}} {{python}} scripts/android_run.py {{args}}

# Build, install and open on the running emulator or connected phone, keeping
# the app's data.
android-install:
    {{android_shell}} {{python}} scripts/android_run.py --running

# Run JavaScript in the open app and print the result (debug builds only).
# Passed in an environment variable, so no shell has to quote it.
android-eval $EPICYCLE_JS:
    {{android_shell}} {{python}} scripts/android_eval.py

# Restart the running emulator or connected phone, e.g. to check that reminders
# survive it.
android-reboot:
    {{android_shell}} adb reboot

# Start the Android emulator (a virtual Pixel 10 with Android 16), then use
# `just android-dev` in another terminal. Arguments go to the emulator. Needs
# KVM on Linux, the hypervisor on Windows.
[unix]
android-emulator *args:
    nix run .#android-emulator -- {{args}}

[windows]
android-emulator *args:
    python nix/android_emulator.py {{args}}

# Run on a connected Android phone (USB debugging on) or a running emulator.
android-dev:
    {{android_shell}} cargo tauri android dev

# Build a debug APK, signed with this machine's Android debug key.
android-debug:
    {{android_shell}} cargo tauri android build --debug --apk --target aarch64

# Build a release APK, signed if crates/app/gen/android/keystore.properties exists.
android-release:
    {{android_shell}} cargo tauri android build --apk --target aarch64

# Create the release signing key, once (see scripts/android_keystore.py).
android-keystore:
    {{android_shell}} {{python}} scripts/android_keystore.py
