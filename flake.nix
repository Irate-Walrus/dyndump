{
  inputs = {
    nixpkgs.url = "github:cachix/devenv-nixpkgs/rolling";
    systems.url = "github:nix-systems/default";
    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs =
    {
      self,
      nixpkgs,
      devenv,
      flake-utils,
      ...
    }@inputs:
    let
      manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
      dyndump =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          inherit (manifest.package) name version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
        };

      flakeForSystem =
        nixpkgs: system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          dyndumpPkg = dyndump pkgs;
        in
        {

          # `nix build #dyndump`
          packages = rec {
            devenv-up = self.devShells.${system}.default.config.procfileScript;
            dyndump = dyndumpPkg;
            default = dyndump;
          };

          # `nix develop`
          devShells.default = devenv.lib.mkShell {
            inherit inputs pkgs;
            modules = [
              {
                enterShell = ''
                  echo $GREET
                  rustc --version
                  export PATH=./result/bin:$PATH
                '';
                name = "dyndump";

                # https://devenv.sh/basics/
                env = {
                  GREET = "🛠️  Entering dev env 🕵️";
                  RUST_BACKTRACE = 1;
                };

                packages = with pkgs; [
                  nixfmt-rfc-style
                  bat
                  jq
                  tealdeer
                  pkg-config
                  openssl
                ];

                languages = {
                  rust = {
                    enable = true;

                    # https://devenv.sh/reference/options/#languagesrustchannel
                    channel = "nightly";

                    components = [
                      "rustc"
                      "cargo"
                      "clippy"
                      "rustfmt"
                      "rust-analyzer"
                    ];
                  };
                };

                # https://devenv.sh/pre-commit-hooks/
                pre-commit.hooks = {
                  nixfmt-rfc-style = {
                    enable = true;
                    package = pkgs.nixfmt-rfc-style;
                  };

                  yamllint = {
                    enable = true;
                    settings.preset = "relaxed";
                  };

                  editorconfig-checker.enable = true;
                };

                # Make diffs fantastic
                difftastic.enable = true;
              }
            ];
          };
          apps = rec {
            dyndump = flake-utils.lib.mkApp {
              drv = dyndumpPkg;
              name = "dyndump";
            };
            default = dyndump;
          };
        };
    in
    flake-utils.lib.eachDefaultSystem (system: flakeForSystem nixpkgs system);
}
