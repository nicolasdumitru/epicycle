{
  description = "Epicycle - local-first subscription tracker";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config.allowUnfree = true;
        };

        # Use the default profile.
        # "The default profile includes all of components in the minimal profile, and adds
        # rust-docs, rustfmt, and clippy. This profile will be used by rustup by default, and it’s
        # the one recommended for general use."
        # https://rust-lang.github.io/rustup/concepts/profiles.html
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-analyzer"
            "rust-src"
          ];
        };

        # System libraries Tauri 2 needs on Linux (WebKitGTK webview + GTK).
        # https://v2.tauri.app/start/prerequisites/#linux
        tauriLibs = with pkgs; [
          at-spi2-atk
          atkmm
          cairo
          gdk-pixbuf
          glib
          glib-networking
          gtk3
          harfbuzz
          librsvg
          libsoup_3
          openssl
          pango
          webkitgtk_4_1
        ];
        epicycle = pkgs.callPackage ./nix/package.nix { };

        # Everything for desktop work. The Android shell adds to this, so the two
        # never drift apart.
        commonPackages = with pkgs; [
          # Tauri
          cargo-tauri
          pkg-config
          gobject-introspection

          # Frontend
          nodejs
          pnpm

          # Database migrations and offline query metadata for sqlx
          sqlx-cli

          # Scripts and the task runner (see justfile)
          # websocket-client is for scripts/android_eval.py.
          (python3.withPackages (p: [ p.websocket-client ]))
          just

          # Nix
          nil
          nixd
          nixfmt
          nixfmt-tree
        ];

        commonShellHook = ''
          # Lets the webview find TLS support (needed for HTTPS requests).
          export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
          # GTK's file chooser (used by the dialog plugin) aborts without its
          # GSettings schemas. Nix packages get these from wrapGAppsHook; the
          # dev shell has to point at them itself.
          export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
        '';

        # Android: the SDK pieces Tauri's Android project needs, pinned so
        # builds don't change under us. The app compiles against SDK 37 with
        # build-tools 37 (see crates/app/gen/android/app/build.gradle.kts);
        # Tauri's own Android library and plugins still want SDK 36 and
        # build-tools 36, so both are included. Gradle can't download missing
        # pieces: the SDK lives in the read-only Nix store.
        androidBuildTools = "37.0.0";
        androidNdk = "29.0.14206865";
        androidenv = pkgs.androidenv.override { licenseAccepted = true; };
        androidComposition = androidenv.composeAndroidPackages {
          platformVersions = [
            "36"
            "37"
          ];
          buildToolsVersions = [
            "36.0.0"
            androidBuildTools
          ];
          includeNDK = true;
          ndkVersions = [ androidNdk ];
          cmdLineToolsVersion = "22.0";
        };
        androidSdk = androidComposition.androidsdk;
        androidHome = "${androidSdk}/libexec/android-sdk";

        # The desktop toolchain plus Rust's Android targets.
        androidRustToolchain = rustToolchain.override {
          targets = [
            "aarch64-linux-android"
            "armv7-linux-androideabi"
            "i686-linux-android"
            "x86_64-linux-android"
          ];
        };
      in
      {
        # `nix run`, `nix build`, or `inputs.epicycle.packages.<system>.default`
        # in a NixOS configuration.
        packages = {
          inherit epicycle;
          default = epicycle;
        }
        // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # An Android phone emulator, a Pixel 10 with Android 16:
          # `nix run .#android-emulator` (see nix/android_emulator.py). Separate
          # from the Android shell so CI doesn't download the phone image.
          # nixpkgs' emulateApp isn't used: it pins command-line tools too old
          # to know recent phones. Needs KVM.
          android-emulator =
            let
              sdk =
                (androidenv.composeAndroidPackages {
                  cmdLineToolsVersion = "22.0";
                  includeEmulator = true;
                  includeSystemImages = true;
                  platformVersions = [ "36" ];
                  systemImageTypes = [ "google_apis" ];
                  abiVersions = [ "x86_64" ];
                }).androidsdk;
            in
            pkgs.writers.writePython3Bin "epicycle-android-emulator"
              {
                # Long command lines read better unwrapped.
                flakeIgnore = [ "E501" ];
              }
              (
                builtins.replaceStrings [ "@sdk@" ] [ "${sdk}/libexec/android-sdk" ] (
                  builtins.readFile ./nix/android_emulator.py
                )
              );
        };

        devShells = {
          # Desktop work: `nix develop` (or direnv).
          default = pkgs.mkShell {
            packages = [ rustToolchain ] ++ commonPackages;
            buildInputs = tauriLibs;
            shellHook = commonShellHook;
          };

          # Everything in the default shell, plus Android: `nix develop .#android`.
          android = pkgs.mkShell {
            packages = [
              androidRustToolchain
              androidSdk
              pkgs.jdk25
            ]
            ++ commonPackages;
            buildInputs = tauriLibs;

            ANDROID_HOME = androidHome;
            ANDROID_SDK_ROOT = androidHome;
            NDK_HOME = "${androidHome}/ndk/${androidNdk}";
            JAVA_HOME = pkgs.jdk25.home;
            # Gradle downloads its own aapt2, a generic Linux binary that can't
            # run on NixOS; use the SDK's instead.
            GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${androidHome}/build-tools/${androidBuildTools}/aapt2";

            shellHook = commonShellHook;
          };
        };
      }
    );
}
