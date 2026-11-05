Manage Rust dependencies with Cargo and use a Nix flake for all of the other dependencies.
Use Conventional Commits and an Angular-inspired convention.

Before starting work, read docs/spec.md (what the app is), docs/status.md (progress, decisions since the spec, next steps) and docs/DECISIONS.md. Update docs/status.md at the end of each piece of work.

## Working

- Ask before making decisions that are expensive to reverse (stack, storage format, backup schema).
- When unsure whether something belongs in the MVP, assume it doesn't and ask.
- Work in small, reviewable increments; keep commits focused.
- Explain non-obvious design choices briefly in code comments or docs/DECISIONS.md.

## Working with the user

- Commit on a feature branch as you go. Push, merge, tag and release only when the user asks.
- The Android signing key and its password are the user's secrets: never in chat or in the repository, only in GitHub secrets and git-ignored local files.
- No Apple ID in CI, and no third-party sideloading tools such as SideStore.
- The user's emulator and its data are theirs. Test on a separate one (see "Testing") unless asked otherwise.
- Say exactly what was tested and how. A passing emulator run isn't a real device, and a debug build isn't a release build: name what is still untested instead of calling it done.

## Testing

Run `just check` and `just test` before committing. Unit tests don't prove anything that depends on a platform or a plugin, so also run the app and look.

- **UI, in a browser:** build the frontend, serve `ui/build` with unknown paths falling back to `index.html`, and load a script before the app that defines `window.__TAURI_INTERNALS__` with an `invoke` answering each command with sample data. Screenshot with headless Chromium (`nix run --inputs-from . nixpkgs#chromium`). Its window can't go below 500 px, so use an iframe for phone widths. Check dark mode too.
- **Android:** `just android-run` builds and opens the current code, `just android-eval` drives the app through its own commands, and `adb shell dumpsys alarm` and `dumpsys notification --noredact` show what is scheduled and delivered. A fresh install stays silent: reminders need notifications allowed, a subscription with something due, and a reminder time still ahead.
- **A separate emulator:** `nix build .#android-emulator -o <temp>/emulator`, then run it with `HOME=<temp>/home` and `-no-window -no-audio -no-snapshot`. If the user's is open too, set `ANDROID_SERIAL` and use `adb` and `android_eval.py` directly, since `just android-run` won't choose between two.
- **Reminders:** docs/spec.md, "Notifications", sets the bar. After updating the notification plugin, check that reminders survive a reboot with the app unopened (see docs/DECISIONS.md, "Reminders").

## Code

- Functional style: algebraic data types, pure functions, immutable data and explicit error types. No class-heavy OOP architecture.
- Keep it simple: no generic plugin frameworks, elaborate dependency injection, event buses, microservices or speculative abstraction layers.
- Write scripts in Python 3, not shell.

## Dependencies and quality

- Don't reinvent the wheel: use established, widely used crates and packages. Avoid obscure ones with few users or with known security issues.
- Keep the dependency count moderate and prioritise security updates.
- Pin dependencies and commit lockfiles.
- The Nix flake provides a reproducible dev environment and will build the package on NixOS.
- CI runs the domain tests, type checks and frontend tests. Automate desktop builds where practical; mobile builds are tested manually before releases.
