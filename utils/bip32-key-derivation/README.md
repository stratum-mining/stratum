# Bip32 Key Derivation

## Binary
It can be used as binary to derive child public keys from a BIP32 Master Public Key, specifying the derivation path: 
`cargo run "vpub5ZMie86usV2ZSrvUSoc3sLg9YM8cmE4xHVzhXudJhezGGoXQ8L6Hash7E4ucffBKZXXi4r5wLiCeouB4sTwSDkfivsbmFAGqvAv9Vt7k7Lg" "m/0/0"`

## Library
It can be imported by other applications that need to derive child public keys from a BIP32 Master Public Key exported from a wallet.

