# The dev shells' tools and their versions, as data, for Windows, where Nix
# doesn't run. scripts/toolchain.py writes this to toolchain.json (adding the
# Windows downloads' checksums) and rust-toolchain.toml, and
# scripts/windows_env.py installs from those. Everything here comes from the
# same packages and settings as the dev shells, so the two can't drift apart.
# See docs/DECISIONS.md, "Windows".
{
  lib,
  rustToolchain,
  rustExtensions,
  androidTargets,
  nodejs,
  pnpm,
  just,
  python3,
  pythonPackages,
  cargo-tauri,
  sqlx-cli,
  jdk,
  androidComposition,
  emulatorComposition,
  emulatorImage,
  androidCmdLineTools,
  androidRepo,
}:
let
  # The archive Google publishes for 64-bit Windows, or one for every system.
  windowsArchive =
    info:
    let
      fits = os: a: (a.os or "all") == os && builtins.elem (a.arch or "all") [ "x64" "all" ];
      found = builtins.filter (fits "windows") info.archives ++ builtins.filter (fits "all") info.archives;
    in
    if found == [ ] then
      throw "Google publishes no Windows download of ${info.path} ${info.revision}"
    else
      builtins.head found;

  # Google's description of each package a composition installs (each source
  # archive keeps it), with the Windows download instead of the Linux one. The
  # rest is what scripts/windows_env.py needs to write package.xml, as nixpkgs
  # does.
  androidPackages =
    components:
    map (
      info:
      removeAttrs info [
        "archives"
        "last-available-day"
      ]
      // {
        archive = windowsArchive info;
      }
    ) (builtins.concatMap (component: map (source: source.info) component.src) components);

  sdkPackages = androidPackages (
    [
      androidComposition.platform-tools
      androidComposition.androidsdk
    ]
    ++ androidComposition.build-tools
    ++ androidComposition.platforms
    ++ androidComposition.ndk-bundles
  );
  emulatorPackages = androidPackages (
    [ emulatorComposition.emulator ] ++ emulatorComposition.system-images
  );

  # The licences these packages are under. The flake accepts them
  # (licenseAccepted); Gradle looks for each one's hash in licenses/.
  licenses = lib.genAttrs (lib.unique (map (p: p.license) (sdkPackages ++ emulatorPackages))) (
    name:
    let
      texts = androidRepo.licenses.${name};
    in
    {
      inherit texts;
      hashes = map (builtins.hashString "sha1") texts;
    }
  );
in
{
  rust = {
    inherit (rustToolchain) version;
    # As in flake.nix: rust-overlay's default profile plus these.
    profile = "default";
    components = rustExtensions;
    inherit androidTargets;
  };

  # Rust tools, installed on Windows with `cargo install --locked`.
  crates = {
    tauri-cli = {
      inherit (cargo-tauri) version;
      features = cargo-tauri.buildFeatures or [ ];
      noDefaultFeatures = cargo-tauri.buildNoDefaultFeatures or false;
    };
    sqlx-cli = {
      inherit (sqlx-cli) version;
      features = sqlx-cli.buildFeatures or [ ];
      noDefaultFeatures = sqlx-cli.buildNoDefaultFeatures or false;
    };
  };

  node = nodejs.version;
  pnpm = pnpm.version;
  just = just.version;
  python = {
    inherit (python3) version;
    packages = lib.listToAttrs (
      map (p: lib.nameValuePair p.pname p.version) (pythonPackages python3.pkgs)
    );
  };
  jdk = {
    inherit (jdk) version;
    # Adoptium's release (e.g. jdk-25.0.4.1+1) is in the download's address.
    linuxUrl = builtins.head (builtins.head jdk.srcs).urls;
  };

  android = {
    inherit licenses;
    cmdlineTools = androidCmdLineTools;
    packages = sdkPackages;
    emulator = {
      image = "system-images;android-${emulatorImage.platform};${emulatorImage.type};${emulatorImage.abi}";
      packages = emulatorPackages;
    };
  };
}
