# RFC

## Add new Sv2 sub(protocol) coinbase negotiation as an Sv2 extension

### Why
Schnorr signatures + presigned txs (today) and CTV (maybe in the near future) make it easier the
implementation of noncustodial pool. In Sv2 there is the possibility to add extensions.
I think that negotiating a coinbase tx between pool and downstreams could be a good use case for an
extension. It also make sense for coinbase negotiators to use the already available Sv2 data format,
framing, handshake, authorization and encryption layer.

### Why I would like to add it to the POC:
1. make Sv2 more interesting so is one more reason to push for adoption
2. in order to do the POC we need to deploy a pool, using a non custodial pool and not having to
   worry about custody bitcoin will make things a lot easier for us.

### Goals:
1. start exploring possible ways to have non custodial pools

### Non goals:
1. formally define the coinbase negotiator role and the coinbase negotiation subprottocol

### Some questions:
1. is the use of extension + flags consistent with how extension have been thought in Sv2?
2. is the proposed use of MuSig1 correct?

### Proposal
Add extension [TODO] used by pool's client (miners) and pool to negotiate a coinbase tx that will be
used in the next block. The extension do not use channels.

SetupConnection.Success.flags:
```
bit  a  : signal if downstream want to negotiate a coinbase or if it will accept any coinbase tx
          proposed
bit  a+1: understand Schnorr + presigned txs negotiation
bit  a+2: understand CTV negotiation 
```

### Schnorr + presigned txs negotiation:
1. Each client send to the pool a public key and the address where it want to retrieve the coins
2. The pool create coinbase that sent the input to an address (a1) obtained from the "sum" of all
   the client's pub key
3. The pool create a transaction for each client. The created tx send an amount from a1 to the address
   provided by the client in step 1
4. Each client sign each transaction created in step 3
5. coinbase from step 2 is used in the next block

In order to calculate the amount that the presigned transaction must transfer from a1 to the client
address in step 2, the pool just use the already provided hashrate by the client that has not been
paid yet (so what the miner is being paid is not the block that contribute to mine but the previous
one/ones)

#### Pros:
1. pool is noncustodial
2. miners can verify the 2 below prop of the coinbase tx that they are mining (a1 is the output of the coinbase tx)
    1. they can see that a valid transaction from a1 to the miner address exist and have the *right*
       amount
    2. they need to sign each valid tx from a1 so they can see that the total amount of all the
       presigned txs from a1 is (coinbase input - the fair reward that they expect)

#### Cons:
1. not scale  well on chain, cause if you are a small miner and you get paid at each found
   blocks you will likely end up paying a lot in fees. You can mitigate it (1) sending the txs when the
   fee is small, (2) aggregating txs, (3) the pool is not paying you at each founded block but only
   when you will be paid an amount above a minimum one.
2. not scale well off chain: pool need to coordinate all the miners, and everyone need to agree and be
   online if only one fail the process need to be restarted from zero.

### CTV
Using CTV you will have the above pros without the above cons so it make sense to add this
possibility if it will be merged on master.

### No coinbase negotiation
Pool can reserve a path to itself where they do custody bitcoin for miner that prefer to use a custodian pool.

### Messages

#### Valid for both Schnorr and CTV negotiations:

Upstream will use the provided address as receiving address for the downstream path.
```
Client -> Server
NewAddress:
    address: B0255
    
msg_type: 0x77
```

#### Valid for Schnorr negotiations:


Upstream will use the pub keys as Xi and Ri to calculate X and R as described here
https://blog.blockstream.com/en-musig-key-aggregation-schnorr-signatures/ (MuSig paragrapher)
```
Client -> Server
NewPubKeyPair:
    pub_key_x: B0255
    pub_key_r: B0255

msg_type: 0x78
```



Downstream will use X, R, m, and L to sign m as described here
https://blog.blockstream.com/en-musig-key-aggregation-schnorr-signatures/ (MuSig paragrapher)
```
Server -> Client
NewTxToSign:
    x: B0255
    r: B0255
    l: B0255
    m: B064K


msg_type: 0x79
```


Upstream will use the provided si value to calculate s as described here
https://blog.blockstream.com/en-musig-key-aggregation-schnorr-signatures/ (MuSig paragrapher)
```
Client -> Server
NewSignature:
    s: B0255
```


After that a tx has been signed by all the downstream, upstream compute the valid signature s and
send it to the downstream that control the output in m
```
Server -> Client
ValidSignature:
   s: B0255


msg_type: 0x7A
```


When downstream will start to mine a new block it will use the new coinbase tx if it know a valid tx
that spend bitcoin from the coinbase output to the downstream's controlled address, and if the sum
of all the bitcoin spent by the tx signed via NewTxToSign do not exceed the coinbase input
```
Server -> Client
NewCoinbase:
   coinbase: B064K


msg_type: 0x7B
```


### Note 1 possible implementation
Let say that a miner want to start to mine with a pool:
1. miner will send a NewPubKeyPair to the pool.
2. pool will not send a NewCoinbaseTx and the miner should start to mine whatever job is provided
   by the pool.
3. pool and miner will register all the valid shares provided by the miner.
   [Sum(valid shares for job n), Sum(valid shares for job n+1, ...]
4. on NewPrevHash pool send NewTxToSign where it pay to the miner all the provided shares, right now
   the miner is still mining a job the is not paying the miner but on next block will mine a block
   that pay the miner.
5. from this point the pool will keep sending NewTxToSign for every NewPrevHash
6. when miner want to exit it just stop mine new jobs and  wait until the pool find a new block. If the miner go offline pool will stop to put the miner reward in the coinbase tx but as soon as the miner will be online the miner will restart to put miner reward in coinbase.

Fair price:
The miner still trust the pool that the shares are payed a fair price.
