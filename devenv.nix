{ pkgs, lib, config, inputs, ... }:

{
  # https://devenv.sh/packages/
  packages = with pkgs; [
    git
    mpv
    pkg-config
  ];

  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };
}
