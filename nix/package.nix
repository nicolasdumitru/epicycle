# The Epicycle desktop app, built the way nixpkgs builds Tauri apps: the
# cargo-tauri hook runs `cargo tauri build`, which builds the frontend (pnpm,
# with dependencies fetched by Nix) and the Rust app, and installs the files
# from the Debian bundle it makes.
{
  lib,
  stdenv,
  rustPlatform,
  cargo-tauri,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm,
  nodejs,
  pkg-config,
  wrapGAppsHook3,
  glib-networking,
  libsoup_3,
  openssl,
  webkitgtk_4_1,
}:

let
  root = ../.;
  fs = lib.fileset;

  # Only what the build reads, so editing docs or CI doesn't rebuild the app.
  src = fs.toSource {
    inherit root;
    fileset = fs.unions [
      (root + "/Cargo.toml")
      (root + "/Cargo.lock")
      (root + "/.cargo")
      (root + "/.sqlx")
      # The Android project isn't part of the desktop app.
      (fs.difference (root + "/crates") (fs.maybeMissing (root + "/crates/app/gen/android")))
      (fs.difference (root + "/ui") (
        fs.unions [
          (fs.maybeMissing (root + "/ui/node_modules"))
          (fs.maybeMissing (root + "/ui/build"))
          (fs.maybeMissing (root + "/ui/.svelte-kit"))
        ]
      ))
    ];
  };

  # The frontend's dependencies, fetched from its lockfile alone.
  uiLock = fs.toSource {
    root = root + "/ui";
    fileset = fs.unions [
      (root + "/ui/package.json")
      (root + "/ui/pnpm-lock.yaml")
      (root + "/ui/pnpm-workspace.yaml")
    ];
  };

  cargoToml = lib.importTOML (root + "/Cargo.toml");
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "epicycle";
  inherit (cargoToml.workspace.package) version;
  inherit src;

  cargoLock.lockFile = root + "/Cargo.lock";

  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version;
    inherit pnpm;
    src = uiLock;
    fetcherVersion = 5;
    # Update after changing ui/pnpm-lock.yaml: build, and copy the hash Nix
    # reports.
    hash = "sha256-UDTcXN9/LDPhtNNNS5MwZjWA/+57B8v7cR/TqnMx8IE=";
  };
  pnpmRoot = "ui";

  nativeBuildInputs = [
    cargo-tauri.hook
    pnpmConfigHook
    pnpm
    nodejs
    pkg-config
  ]
  ++ lib.optionals stdenv.hostPlatform.isLinux [ wrapGAppsHook3 ];

  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
    # TLS for the webview's GIO; the app's own HTTPS uses rustls.
    glib-networking
    libsoup_3
    openssl
    webkitgtk_4_1
  ];

  # CI runs the tests (`just test`); repeating them here only slows installs.
  doCheck = false;

  meta = {
    description = "Local-first subscription tracker";
    homepage = "https://github.com/nicolasdumitru/epicycle";
    license = lib.licenses.agpl3Plus;
    mainProgram = "epicycle";
    platforms = lib.platforms.linux;
  };
})
