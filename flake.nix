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
      in
      {
        # `nix run`, `nix build`, or `inputs.epicycle.packages.<system>.default`
        # in a NixOS configuration.
        packages = {
          inherit epicycle;
          default = epicycle;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
          ]
          ++ (with pkgs; [
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
            python3
            just

            # Nix
            nil
            nixd
            nixfmt
            nixfmt-tree
          ]);

          buildInputs = tauriLibs;

          shellHook = ''
            # Lets the webview find TLS support (needed for HTTPS requests).
            export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
            # GTK's file chooser (used by the dialog plugin) aborts without its
            # GSettings schemas. Nix packages get these from wrapGAppsHook; the
            # dev shell has to point at them itself.
            export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}''${XDG_DATA_DIRS:+:$XDG_DATA_DIRS}"
          '';
        };
      }
    );
}
