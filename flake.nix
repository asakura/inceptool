{
  description = "inceptool: an extensible LLM agent hook architecture";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      crane,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain (_: rustToolchain);

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          strictDeps = true;
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        inceptool = pkgs.callPackage ./package.nix { inherit crane; };

        # Static, fully self-contained Linux binaries for release artifacts.
        # The *build* platform stays the normal glibc x86_64-linux toolchain;
        # only the final crate output is cross-compiled to a musl target,
        # statically linked, via the matching `pkgsCross` C toolchain as
        # the linker. This avoids `pkgsStatic`, which would also make rustc
        # itself a static-musl binary - that combination crashes proc-macro
        # build scripts.
        mkMuslPackage =
          crossPkgs: targetTriple:
          let
            targetEnvVar = pkgs.lib.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] targetTriple);
            crossCC = "${crossPkgs.stdenv.cc.targetPrefix}cc";
            muslArgs = commonArgs // {
              pname = "inceptool";
              doCheck = false;
              CARGO_BUILD_TARGET = targetTriple;
              "CARGO_TARGET_${targetEnvVar}_LINKER" = crossCC;
              "CC_${builtins.replaceStrings [ "-" ] [ "_" ] targetTriple}" = crossCC;
              RUSTFLAGS = "-C target-feature=+crt-static";
              depsBuildBuild = [ crossPkgs.stdenv.cc ];
            };
            muslCargoArtifacts = craneLib.buildDepsOnly muslArgs;
          in
          craneLib.buildPackage (muslArgs // { cargoArtifacts = muslCargoArtifacts; });
      in
      {
        packages = {
          inherit inceptool;
          default = inceptool;
        }
        // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          inceptool-x86_64-linux-musl = mkMuslPackage pkgs.pkgsCross.musl64 "x86_64-unknown-linux-musl";
          inceptool-aarch64-linux-musl = mkMuslPackage pkgs.pkgsCross.aarch64-multiplatform-musl "aarch64-unknown-linux-musl";
        };

        checks = {
          fmt = craneLib.cargoFmt { inherit src; };
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--workspace --all-targets -- -D warnings";
            }
          );
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ inceptool ];
          RUSTFLAGS = "";
          packages = with pkgs; [
            rustToolchain
            git
            pre-commit
            cargo-deny
            cargo-llvm-cov
            cargo-nextest
            git-cliff
            nixfmt-rfc-style
            shfmt
            shellcheck
            rtk
          ];
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
