
<h1 align="center">
  <br>
  <a href="https://stratumprotocol.org"><img src="https://github.com/stratum-mining/stratumprotocol.org/blob/660ecc6ccd2eca82d0895cef939f4670adc6d1f4/src/.vuepress/public/assets/stratum-logo%402x.png" alt="SRI" width="200"></a>
  <br>
Stratum V2 Reference Implementation (SRI)
  <br>
</h1>

<h4 align="center">SRI is a reference implementation of the Stratum V2 protocol written in Rust 🦀.</h4>

<p align="center">
  <a href="https://codecov.io/gh/stratum-mining/stratum">
    <img src="https://codecov.io/gh/stratum-mining/stratum/branch/main/graph/badge.svg" alt="codecov">
  </a>
  <a href="https://twitter.com/intent/follow?screen_name=stratumv2">
    <img src="https://img.shields.io/twitter/follow/stratumv2?style=social" alt="X (formerly Twitter) Follow">
  </a>
</p>

## 💼 Table of Contents

<p align="center">
  <a href="#-introduction">Introduction</a> •
  <a href="#%EF%B8%8F-getting-started">Getting Started</a> •
  <a href="#-use-cases">Use Cases</a> •
  <a href="#-roadmap">Roadmap</a> •
  <a href="#-contribute">Contribute</a> •
  <a href="#-support">Support</a> •
  <a href="#-donate">Donate</a> •
  <a href="#-supporters">Supporters</a> •
  <a href="#-license">License</a> 
  <a href="#-msrv">MSRV</a>
</p>

## 👋 Introduction

Welcome to the official GitHub repository for the **SRI - Stratum V2 Reference Implementation**. 

[Stratum V2](https://stratumprotocol.org) is a next-generation bitcoin mining protocol designed to enhance the efficiency, security, flexibility and decentralization. 
SRI is fully open-source, community-developed, independent of any single entity, aiming to be fully compatible with [Stratum V2 Specification](https://github.com/stratum-mining/sv2-spec).

## ⛏️ Getting Started

To get started with the Stratum V2 Reference Implementation (SRI), please follow the detailed setup instructions available on the official website:

[Getting Started with Stratum V2](https://stratumprotocol.org/blog/getting-started/)

This guide provides all the necessary information on prerequisites, installation, and configuration to help you begin using, testing or contributing to SRI.

## 🚀 Use Cases

The library is modular to address different use-cases and desired functionality. Examples include:

### 👷 Miners

- SV1 Miners can use the translator proxy (`roles/translator`) to connect with a SV2-compatible pool.
- SV1 mining farms mining to a SV2-compatible pool gain some of the security and efficiency improvements SV2 offers over Stratum V1 (SV1). The SV1<->SV2 translator proxy does not support  _all_ the features of SV2, but works as a temporary measure before upgrading completely to SV2-compatible firmware. (The SV1<->SV2 translation proxy implementation is a work in progress.)

### 🛠️ Pools

- Pools supporting SV2 can deploy the open source binary crate (`roles/pool`) to offer their clients (miners participating in said pool) an SV2-compatible pool.
- The Rust helper library provides a suite of tools for mining pools to build custom SV2 compatible pool implementations.

## 🛣 Roadmap 

Our roadmap is publicly available, outlining current and future plans. Decisions on the roadmap are made through a consensus-driven approach, through participation on dev meetings, Discord or GitHub.

[View the SRI Roadmap](https://github.com/orgs/stratum-mining/projects/5)

### 🏅 Project Maturity

Low-level crates (`protocols` directory) are considered **beta** software. Rust API Docs is a [work-in-progress](https://github.com/stratum-mining/stratum/issues/845), and the community should still expect small breaking API changes and patches.

Application-level crates (`roles` directory) are considered **alpha** software, and bugs are expected. They should be used as a guide on how to consume the low-level crates as dependencies.

### 🎯 Goals

The goals of this project are to provide:

1. A robust set of Stratum V2 (SV2) primitives as Rust library crates which anyone can use
   to expand the protocol or implement a role. For example:
   - Pools supporting SV2
   - Mining-device/hashrate producers integrating SV2 into their firmware
   - Bitcoin nodes implementing Template Provider to build the `blocktemplate`
2. A set of helpers built on top of the above primitives and the external Bitcoin-related Rust crates for anyone to implement the SV2 roles.
3. An open-source implementation of a SV2 proxy for miners.
4. An open-source implementation of a SV2 pool for mining pool operators.

## 💻 Contribute 

If you are a developer looking to help, but you're not sure where to begin, check the [good first issue label](https://github.com/stratum-mining/stratum/labels/good%20first%20issue), which contains small pieces of work that have been specifically flagged as being friendly to new contributors.

Contributors looking to do something a bit more challenging, before opening a pull request, please join [our community chat](https://discord.gg/fsEW23wFYs) or [start a GitHub issue](https://github.com/stratum-mining/stratum/issues) to get early feedback, discuss the best ways to tackle the problem, and ensure there is no work duplication and consensus.

## 🤝 Support

Join our Discord community to get help, share your ideas, or discuss anything related to Stratum V2 and its reference implementation. 

Whether you're looking for technical support, want to contribute, or are just interested in learning more about the project, our community is the place to be.

[Join the Stratum V2 Discord Community](https://discord.gg/fsEW23wFYs)

## 🎁 Donate

### 👤 Individual Donations 
If you wish to support the development and maintenance of the Stratum V2 Reference Implementation, individual donations are greatly appreciated. You can donate through OpenSats, a 501(c)(3) public charity dedicated to supporting open-source Bitcoin projects.

[Donate through OpenSats](https://opensats.org/projects/stratumv2)

### 🏢 Corporate Donations
For corporate entities interested in providing more substantial support, such as grants to SRI contributors, please get in touch with us directly. Your support can make a significant difference in accelerating development, research, and innovation.

Email us at: stratumv2@gmail.com

## 🙏 Supporters

SRI contributors are independently, financially supported by following entities: 

<p float="left">
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
> Twitter [@Stratumv2](https://twitter.com/StratumV2)
