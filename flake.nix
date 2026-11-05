{
  description = "eza path colors for fish, with an optional versioned provider hook";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/70fdef7c16ffb24c9c74d91c73e438a39b46bfb5";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = import ./default.nix { inherit pkgs; };
          fish = pkgs.fish.overrideAttrs (old: {
            patches = (old.patches or []) ++ [ ./compat/fish-4.9.3/path-style-provider.patch ];
            postPatch = (old.postPatch or "") + ''
              cp ${./compat/fish-4.9.3/path-style-provider.rs} src/highlight/path_style_provider.rs
            '';
          });
        });
      checks = forAllSystems (system: {
        provider = self.packages.${system}.default;
      });
    };
}
