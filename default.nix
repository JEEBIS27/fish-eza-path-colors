{ pkgs ? import <nixpkgs> {} }:
pkgs.rustPlatform.buildRustPackage {
  pname = "fish-eza-path-colors";
  version = "0.1.0";
  src = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter = path: type:
      let name = baseNameOf path;
      in pkgs.lib.cleanSourceFilter path type
        && name != "target" && name != "result" && !(pkgs.lib.hasPrefix "result-" name);
  };
  cargoLock.lockFile = ./Cargo.lock;
  EZA_EXECUTABLE = "${pkgs.eza}/bin/eza";
  postInstall = ''
    mkdir -p "$out/conf.d"
    cp conf.d/fish-eza-path-colors.fish "$out/conf.d/"
    substituteInPlace "$out/conf.d/fish-eza-path-colors.fish" \
      --replace-fail '(command -s fish-eza-path-colors-provider)' '"'"$out"'/bin/fish-eza-path-colors-provider"'
  '';
  meta = {
    description = "Optional eza path-style provider and fish plugin";
    license = pkgs.lib.licenses.mit;
    platforms = pkgs.lib.platforms.unix;
    mainProgram = "fish-eza-path-colors-provider";
  };
}
