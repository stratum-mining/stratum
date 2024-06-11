
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = [ pkgs.cpuminer pkgs.rustc pkgs.cargo pkgs.libiconv pkgs.glow ];
}

