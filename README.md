<h1 align="center">
  <br>
  <a href="https://stratumprotocol.org"><img src="https://github.com/stratum-mining/stratumprotocol.org/blob/660ecc6ccd2eca82d0895cef939f4670adc6d1f4/src/.vuepress/public/assets/stratum-logo%402x.png" alt="SRI" width="200"></a>
  <br>
SV2 Libraries
  <br>
</h1>
<h4 align="center">Stratum V2 protocol libraries from the SRI project 🦀</h4>
<p align="center">
  <a href="https://codecov.io/gh/stratum-mining/stratum">
    <img src="https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg" alt="codecov">
  </a>
  <a href="https://twitter.com/intent/follow?screen_name=stratumv2">
    <img src="https://img.shields.io/twitter/follow/stratumv2?style=social" alt="X (formerly Twitter) Follow">
  </a>
</p>

# Stratum Repository

This repository contains the low-level crates.
If you’re looking to run Sv2 applications at the most recent changes, check out the [`sv2-apps` repository](https://github.com/stratum-mining/sv2-apps). Those crates are application-level, currently in **alpha** stage.

## Contents

- `sv1/` - Stratum V1 protocol implementation and utilities
- `sv2/` - Stratum V2 protocol implementations
  - `binary-sv2/` - Binary encoding/decoding for SV2 messages
  - `buffer/` - Buffer management and pooling
  - `channels-sv2/` - Channel management for SV2
  - `codec-sv2/` - SV2 message codec with encryption support
  - `framing-sv2/` - SV2 message framing utilities
  - `handlers_sv2/` - SV2 message handler traits
  - `noise-sv2/` - Noise protocol implementation for SV2
  - `parsers-sv2/` - Message parsing utilities
  - `subprotocols/` - SV2 subprotocol implementations
- `stratum-core/` - Entrypoint for all the low-level crates in `sv2/` and `sv1/`implementations
  - `stratum-translation` - Stratum V1 ↔ Stratum V2 translation utilities

## Local Integration Testing

To run integration tests locally:

```bash
./scripts/run-integration-tests.sh
```

This will:
1. Clone/update the integration test framework
2. Update dependencies to use your local changes
3. Run the full integration test suite
4. Restore the original configuration

## 🛣 Roadmap 

Our roadmap is publicly available as part of the broader SRI project, outlining current and future plans. Decisions are made through a consensus-driven approach via dev meetings, Discord, and GitHub.

[View the SRI Roadmap](https://github.com/orgs/stratum-mining/projects/15)

## 💻 Contribute 

We welcome contributions to improve these SV2 libraries! Here's how you can help:

1. **Start small**: Check the [good first issue label](https://github.com/stratum-mining/stratum/labels/good%20first%20issue) in the main SRI repository
2. **Join the community**: Connect with us on [Discord](https://discord.gg/fsEW23wFYs) before starting larger contributions
3. **Open issues**: [Create GitHub issues](https://github.com/stratum-mining/stratum/issues) for bugs, feature requests, or questions
4. **Follow standards**: Ensure code follows Rust best practices and includes appropriate tests

## 🤝 Support

Join our Discord community for technical support, discussions, and collaboration:

[Join the Stratum V2 Discord Community](https://discord.gg/fsEW23wFYs)

For detailed documentation and guides, visit:
[Stratum V2 Documentation](https://stratumprotocol.org)

## 🎁 Donate

### 👤 Individual Donations 
If you wish to support the development and maintenance of the Stratum V2 Reference Implementation, individual donations are greatly appreciated. You can donate through OpenSats, a 501(c)(3) public charity dedicated to supporting open-source Bitcoin projects.

[Donate through OpenSats](https://opensats.org/projects/stratumv2)

### 🏢 Corporate Donations
For corporate entities interested in providing more substantial support, such as grants to SRI contributors, please get in touch with us directly. Your support can make a significant difference in accelerating development, research, and innovation.

## 🙏 Supporters

SRI contributors are independently, financially supported by following entities: 

<p float="left">
  <a href="https://btrust.tech"><img src="https://raw.githubusercontent.com/stratum-mining/stratumprotocol.org/refs/heads/main/public/assets/btrust-boxed.png" width="250" /></a>
  <a href="https://hrf.org"><img src="https://raw.githubusercontent.com/stratum-mining/stratumprotocol.org/refs/heads/main/public/assets/hrf-logo-boxed.svg" width="250" /></a>
  <a href="https://spiral.xyz"><img src="https://raw.githubusercontent.com/stratum-mining/stratumprotocol.org/refs/heads/main/public/assets/Spiral-logo-boxed.svg" width="250" /></a>
  <a href="https://opensats.org/"><img src="https://raw.githubusercontent.com/stratum-mining/stratumprotocol.org/refs/heads/main/public/assets/opensats-logo-boxed.svg" width="250" /></a>
  <a href="https://vinteum.org/"><img src="https://raw.githubusercontent.com/stratum-mining/stratumprotocol.org/refs/heads/main/public/assets/vinteum-logo-boxed.png" width="250" /></a>
</p>

## 📖 License
This software is licensed under Apache 2.0 or MIT, at your option.

## 🦀 MSRV
Minimum Supported Rust Version: 1.75.0

---

> Website [stratumprotocol.org](https://www.stratumprotocol.org) &nbsp;&middot;&nbsp;
> Discord [SV2 Discord](https://discord.gg/fsEW23wFYs) &nbsp;&middot;&nbsp;
> Twitter [@Stratumv2](https://twitter.com/StratumV2) &nbsp;&middot;&nbsp;