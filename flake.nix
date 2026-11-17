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
        rustExtensions = [
          "rust-analyzer"
          "rust-src"
        ];
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = rustExtensions;
        };

        # Python and the packages the scripts need. websocket-client is for
        # scripts/android_eval.py.
        pythonPackages = p: [ p.websocket-client ];

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
          (python3.withPackages pythonPackages)
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
        androidCmdLineTools = "22.0";
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
          cmdLineToolsVersion = androidCmdLineTools;
        };
        androidSdk = androidComposition.androidsdk;
        androidHome = "${androidSdk}/libexec/android-sdk";

        # The emulator's phone image: Android 16 with Google APIs.
        emulatorImage = {
          platform = "36";
          type = "google_apis";
          abi = "x86_64";
        };
        emulatorComposition = androidenv.composeAndroidPackages {
          cmdLineToolsVersion = androidCmdLineTools;
          includeEmulator = true;
          includeSystemImages = true;
          platformVersions = [ emulatorImage.platform ];
          systemImageTypes = [ emulatorImage.type ];
          abiVersions = [ emulatorImage.abi ];
        };
        emulatorSdk = emulatorComposition.androidsdk;

        # Adoptium's Temurin build rather than nixpkgs' own OpenJDK: Adoptium
        # publishes the same release for Windows, so both use one JDK (see
        # nix/toolchain.nix).
        jdk = pkgs.temurin-bin-25;

        # The desktop toolchain plus Rust's Android targets.
        androidTargets = [
          "aarch64-linux-android"
          "armv7-linux-androideabi"
          "i686-linux-android"
          "x86_64-linux-android"
        ];
        androidRustToolchain = rustToolchain.override {
          targets = androidTargets;
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
            pkgs.writers.writePython3Bin "epicycle-android-emulator"
              {
                # Long command lines read better unwrapped.
                flakeIgnore = [ "E501" ];
              }
              (
                builtins.replaceStrings
                  [ "@sdk@" "@image@" "@cmdline_tools@" ]
                  [
                    "${emulatorSdk}/libexec/android-sdk"
                    "system-images;android-${emulatorImage.platform};${emulatorImage.type};${emulatorImage.abi}"
                    androidCmdLineTools
                  ]
                  (builtins.readFile ./nix/android_emulator.py)
              );
        };

        # The dev shells' tool versions, for Windows, where Nix doesn't run:
        # `nix eval .#toolchain.x86_64-linux`. scripts/toolchain.py turns this
        # into toolchain.json and rust-toolchain.toml.
        toolchain = import ./nix/toolchain.nix {
          inherit (pkgs)
            lib
            nodejs
            pnpm
            just
            python3
            cargo-tauri
            sqlx-cli
            ;
          inherit
            rustToolchain
            rustExtensions
            androidTargets
            pythonPackages
            jdk
            androidComposition
            emulatorComposition
            emulatorImage
            androidCmdLineTools
            ;
          androidRepo = pkgs.lib.importJSON "${pkgs.path}/pkgs/development/mobile/androidenv/repo.json";
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
              jdk
            ]
            ++ commonPackages;
            buildInputs = tauriLibs;

            ANDROID_HOME = androidHome;
            ANDROID_SDK_ROOT = androidHome;
            NDK_HOME = "${androidHome}/ndk/${androidNdk}";
            JAVA_HOME = jdk.home;
            # Gradle downloads its own aapt2, a generic Linux binary that can't
            # run on NixOS; use the SDK's instead.
            GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${androidHome}/build-tools/${androidBuildTools}/aapt2";

            shellHook = commonShellHook;
          };
        };
      }
    );
}
