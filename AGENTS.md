Manage Rust dependencies with Cargo and use a Nix flake for all of the other dependencies.
Use Conventional Commits and an Angular-inspired convention.

Before starting work, read docs/spec.md (what the app is), docs/status.md (progress, decisions since the spec, next steps) and DECISIONS.md. Update docs/status.md at the end of each piece of work.

## Working

- Ask before making decisions that are expensive to reverse (stack, storage format, backup schema).
- When unsure whether something belongs in the MVP, assume it doesn't and ask.
- Work in small, reviewable increments; keep commits focused.
- Explain non-obvious design choices briefly in code comments or DECISIONS.md.

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
