# sv1-to-sv2 Example
Translates sv1 packets sent from a mining device to sv2 packets to be sent to an upstream node
(likely a sv2-compatible pool).

## Implementation Plan
1. Create the translation using Extended Channels with the upstream.
2. Create the translation using Standard Channels with the upstream.
3. Integrate logic from the example into the appropriate libraries.

## Run
```
% cargo run -p sv1_to_sv2
```
