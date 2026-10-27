{
  description = "OxiTal";

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
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
          ]
          ++ (with pkgs; [
            nil
            nixd
            nixfmt
            nixfmt-tree
          ]);
        };
      }
    );
}
