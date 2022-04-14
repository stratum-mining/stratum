# RFC

One of the biggest treat to Bitcoin decentralization are pools. Stratum V2 introduce a way for the
miners to select the transaction that they want to include in the mined block. That make very easy
for the miner to see if the pool is censuring transactions, but alone is not enough, we need to have at least
one pool that is not censuring transactions. 

Best thing that we can do to ensure that there will always be at least one honest pool is to make
as easy as possible deploy a pool (that means maintain an open source implementation), and this is
already in the scope of the stratum-mining project, but is still not enough.
Having an honest pool is pretty useless if it do not have any hashrate, we need to make as easy as
possible for a miner to switch to the honest pool, and that means that the we must minimize the
amount of trust that a miner need to put into the pool operator.

To recap we need:
1. an open source very easy to deploy pool
2. the pool code should be well tested so that miners can trust that the pool actually works
3. miners should place the minimum possible amount of trust into the pool operator (non custodial)

## Non custodial pool
A non custodial pool is pool the do not custody any bitcoin in order to do that the coinbase
output/s of the pool redistribute coinbase reward directly to the miners. 

Below some possible non-custodial pool implementation:
1. classical: coinbase have an output for each miner
2. coinpool: coinpool can be built with (1) schnorr, (2) schnorr + ANYPREVOUT + (TODO), (3) CTV. Each
   possibility will be discussed.
3. channels
4. sidechains

### Minimum trust problem
The minimum trust problem is that the pool in order to know how much a miner should be rewarded need 
to receive shares from the miner.
That means that if ^t is the time between coinbase updates the miner must trust the pool to reward
the miner for the work done in that ^t in the next coinbase. (or the other way around)
A pool colluding with a miner or a group of miner could have an incentive in doing this attack. If a
coinbase is negotiate very often these incentives tend to disappear (TODO prove it)

The minimum trust problem can be solved by decentralized pools (TODO) or using sidechains (TODO)
Decentralized pool do not works for small miners but a scenario where small miner use non-custodial
pools and that non-custodial pool use decentralized pools is possible.

## Kind of non-custodial pools

### classical

TODO

### coinpool with shnorr only


I do want ANYPREVOUT and 

**Initialize the pool**
1. A PubKey is constructed by all the pool participants.
2. We use the above PubKey to build the coinbase output.
3. A tx that spend the above output to each participants (each one with the right quota) is
   constructed.
4. The above tx is signed by all the participants.

```
10 -> ABC
  -> 7 -> A
     2 -> B
     1 -> C
```

**Sum outputs (need cooperation)**
1. The pool create a tx (tx1) that have as inputs two ore more coinbase outputs.
2. Both the aggregated coinbase outputs and the tx1 output have the same pk_script (controlled by
   every participant)
3. A tx (tx2) that spend tx1 output to each participant (each one with the right quota)
4. tx2 is signed by each participant
5. tx1 is signed by each participant
6. tx1 is added to a new block
```
10 -> ABC
  -> 7 -> A
     2 -> B
     1 -> C

10 -> ABD
  -> 7 -> A
     2 -> B
     1 -> D

-> 20 -> ABCD
  -> 14 -> A
     4 -> B
     1 -> C
     1 -> D
```
When we aggregate, we should avoid to end up with txs with to many output. That means that could be
good have more coinpools each one with separate participants:
1. we avoid to end up with very big txs
2. is more robust (if one participant become unresponsive a smaller set of participant is affected)

If a pool have six participant instead of:
```
10 -> ABCDEF
  -> 3 -> A
     2 -> B
     2 -> C
     1 -> D
     1 -> E
     1 -> F
```

Could do:
```
5 -> ABC
 ...
5 -> DEF
 ...
```

**Exit with cooperation**
```
10 -> ABC
  -> 7 -> A
     2 -> B
     1 -> C

-> 3 -> AB
   1 -> C
```

**Exit without cooperation**
```
10 -> ABC
  -> 7 -> A
     2 -> B
     1 -> C

-> 7 -> A
   2 -> B
   1 -> C
```

If one participant is no longer online, presigned txs can be added when blockspace is cheap.

Honest participant do not have any incentive in being uncooperative, btw is a plausible attack for
someone that want to disrupt the service.

### coinpool with shnorr + ANYPREVOUT + (TODO)

TODO

### coinpool with CTV

TODO

### channels

TODO

### sidechains

## Proposal
A non-custodial pool would require the negotiation of a coinbase transaction between the upstream 
Pool Service and the downstream nodes. This is a good use case for an SV2 extension as the coinbase
negotiation process can use the already available data format, framing, handshake, authorization,
and encryption layer as defined by the SV2 protocol.

# Links
[decentralized pools](https://lists.linuxfoundation.org/pipermail/bitcoin-dev/2021-December/019662.html)
[coinpools](https://coinpool.dev/v0.1.pdf)
