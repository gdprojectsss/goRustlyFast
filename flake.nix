{
  description = "grfsh, a small Unix shell with typed table pipelines";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forEachSystem = nixpkgs.lib.genAttrs systems;
    in {
      packages = forEachSystem (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in rec {
          grfsh = pkgs.rustPlatform.buildRustPackage {
            pname = "grfsh";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.installShellFiles ];
            checkFlags = [ "--skip=interactive" ];
            postInstall = "installManPage docs/grfsh.1";
            passthru.shellPath = "/bin/grfsh";
            meta = {
              description = "Small Unix shell with bash style scripting and typed table pipelines";
              homepage = "https://github.com/gdprojectsss/goRustlyFast";
              license = pkgs.lib.licenses.mit;
              mainProgram = "grfsh";
            };
          };
          default = grfsh;
        });

      devShells = forEachSystem (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            packages = [ pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt pkgs.hyperfine ];
          };
        });
    };
}
