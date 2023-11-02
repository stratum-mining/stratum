use std::env;
use stratum_common::bitcoin::util::bip32::ExtendedPubKey;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: cargo run <bip32 master extened public key> <derivation path (m/0/0)>");
        std::process::exit(1);
    }

    let slip132_master_pub_key = &args[1];
    let derivation_path = &args[2];
    let bip32_extended_pub_key: ExtendedPubKey =
        slip132::FromSlip132::from_slip132_str(slip132_master_pub_key).unwrap();
    let child_pub_key =
        bip32_derivation::derive_child_public_key(&bip32_extended_pub_key, derivation_path)
            .unwrap();
    println!(
        "\nPublic key derived from your Master Public Key -> {:?}",
        child_pub_key.to_pub().inner.to_string()
    );
    println!(
        "\nCopy/paste it in your configuration file (filling the output_script_value field)!\n"
    );
}
