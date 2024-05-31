
{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = [ pkgs.cpuminer pkgs.rustc pkgs.cargo pkgs.libiconv pkgs.glow ];
  shellHook = ''
    export pkg_cpuminer="${pkgs.cpuminer}"

    # Run cargo run in the background and redirect its output to a file
    alias tproxy='cargo run --manifest-path roles/translator/Cargo.toml -- -c roles/translator/config-examples/tproxy-config-hosted-pool-example.toml'

    # Start minerd in the background and redirect its output to a file
    alias cpuminer='minerd -a sha256d -o stratum+tcp://localhost:34255 -q -D -P'

    echo -e "Welcome to the BTCPP_SRI nix development shell.\n\
    Wanna do some CPU mining on our testnet pool over SV2?\n\
    \n\
    To start the translator proxy, you can use the 'tproxy' alias, which resolves to: \n\
    $ cargo run --manifest-path roles/translator/Cargo.toml -- -c roles/translator/config-examples/tproxy-config-hosted-pool-example.toml \n\
    \n\
    Then, on a new nix-shell instance, start cpuminer via the 'cpuminer' alias, which resolves to:.\n\
    $ minerd -a sha256d -o stratum+tcp://localhost:34255 -q -D -P
                  ⚠️WARNING ⚠️\n\
        this will consume a lot of CPU power!\n\
       you should monitor the process closely!\n\
    We are not liable for damages to your hardware 🙏\n\
    "
  '';
}

