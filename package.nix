{
  pkgs,
  crane,
  rustToolchain ? pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml,
  craneLib ? crane.mkLib pkgs,
}:
let
  libCrane =
    if rustToolchain != null then craneLib.overrideToolchain (_: rustToolchain) else craneLib;

  commonArgs = {
    # The default cargo-sources filter drops test inputs: parable's build.rs
    # generates its tests from corpus/*.tests, and insta reads *.snap snapshots.
    src = pkgs.lib.cleanSourceWith {
      name = "source";
      src = ./.;

      filter =
        path: type:
        (libCrane.filterCargoSources path type)
        || (pkgs.lib.hasSuffix ".tests" path)
        || (pkgs.lib.hasSuffix ".snap" path);
    };

    strictDeps = true;
  };

  cargoArtifacts = libCrane.buildDepsOnly commonArgs;
in
libCrane.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    pname = "inceptool";

    # Integration tests create throwaway git repositories and shell out to rtk.
    nativeCheckInputs = [
      pkgs.git
      pkgs.rtk
    ];

    meta.mainProgram = "inceptool";
  }
)
