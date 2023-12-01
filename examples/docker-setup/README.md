# Stratum V2 Docker Files 

This examples uses docker and docker-compose to bootstrap the server. Keep in mind that this is working in progress, any suggestion is more than welcome.

Update the config files on conf/ folder.

## Overview

The idea is to run the roles using docker and when neede group them by profile. eg:

  - Miner (as a miner you shouldn't need to run any of it just point to the pool)
    - translator proxy (translator_sv2)
    - template provider (bitcoin core with sv2 support)
    - job declarator client (jd_client)

  - Mining Pool
    - job declarator server (jd_server)
    - translator proxy (translator_sv2)
    - template provider (bitcoin core with sv2 support)
    - pool server (pool_sv2)
  

### How to build the Bitcoin Core with SV2 support image

  $ docker build --file bitcoinsv2.dockerfile . -t bitcoinsv2

### How to build the Stratum V2 image

  $ docker build -t stratumv2 --file build.dockerfile .

### TODO

Potential improvements and features could be:

  - TUI inspired console interface to monitor the containers logs 
  - Rest API to fetch information
  - Monitor profile with more information  

  