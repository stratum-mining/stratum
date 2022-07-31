# SV1 to SV2 Translator Example
Translates sv1 packets sent from a mining device to sv2 packets to be sent to an upstream node
(likely a sv2-compatible pool).

## Run
```
% cargo run -p translator
```


1. first you define the socket where the server will listen for incoming connection:
    https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/main.rs#L166
2. then the server need to bind to a socket and start listening:
   https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/downstream_mining.rs#L318
3. then a client will need to try to connect:
   https://github.com/stratum-mining/stratum/blob/main/roles/v2/test-utils/mining-device/src/main.rs#L16
4. server will open the connection and it will initialize the struct PlainConnection, that will return a Receiver<EitherFrame> and a Sender<EitherFrame> via the sender you can send messages to the client and via the receiver you can parse messages sent by the client.
   https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/downstream_mining.rs#L320-L322
5. you will use the same struct for the client:
   https://github.com/stratum-mining/stratum/blob/main/roles/v2/test-utils/mining-device/src/main.rs#L17-L18


# Initial Considerations
1. Use tokio instead of async_std to mock
2. To mock the Sv1 client, could use the binary in `roles/v1/test-utils/mining-device`
3. Do NOT need to mock the pool because we can use the pool in `roles/v2/pool`

# Example
1. Translator: Opens an extended channel with the pool with the required min extranonce
2. Pool:       Sends `OpenChannelSuccess` + ExtendedJob + prev hash
3. Translator: Begins listening for a downstream (Miner) connection
4. Sv1 Client: Sends `Authorize`
5. Translator: Returns `Ok`
6. Sv1 Client: Sends `Extranonce Subscribe` + `Subscribe`
7. Translator: Sends `server_to_client::Subscribe`. The `extranonce1` field will be the extranonce
               prefix (assigned by the Pool to the Translator in (2)) + a unique nonce that the
               Translator assigned to this particular Miner.
               Translator also sends a `Notify` message that will derive from the extended job and
               prev hash that the Pool sent to the Translator in (2).
8. Sv1 Client: Sends `mining.submit` as soon as a share is found
9. Translator: Transforms the Sv1 `mining.submit` into a `SubmitSharesExtended` and send it to the
               Pool
10. Pool:      Sends Success message to Translator
11. Translator: Sends Success message to the Sv1 Client


# Reason
After that, we have a working example which will be reused to implement the Translator into the
Proxy.

# Macro Tasks
1. Mock the Sv1 Client: This can easily be done by copying the code that is in
                        `roles/v2/test-utils/mining-device` and replacing the Sv2 messages with Sv1
                        message. The biggest difference is that the mock Sv2 Mining Device can
                        change the extranonce, but we can just ignore it and use a fixed extranonce
                        for each job that the Sv1 Mining Device will receive.
2. Implement the Translator example: More below.
3. Add support for Extended Channels into the Pool: Fi3 will do this

# Implement the Translator
We will likely end up with two principal structs:
1. `Downstream` that reads from a socket opened with a Sv1 Client and expects an Sv1 message. Also
    writes Sv1 messages into that socket.
2. `Upstream` does the same for the pool but with Sv2 messages.

## Extend above list with task that Translator needs to do divided by `Downstream` and `Upstream`:
1. Translator opens an Extended Channel with the Pool with the required min extranonce:
  1. `Upstream` opens a TCP connection with the Pool
  2. `Upstream` creates an `OpenExtendedChannelRequest` message and sends it to the Pool

2. Pool sends an `OpenChannelSuccess` + ExtendedJob + prev hash:
  - `Upstream` blocks until Pool sends:
     - `OpenExtendedChannelRequest.Success`
     - `NewExtendedMiningJob`
     - `SetNewPrevHash`

3. Translator starts listening for a downstream (Mining Device) connection
  - As soon as a TCP connection is open on the listening address, Translator instantiates a
     new `Downstream`

4. Sv1 client sends `mining.authorize` message
  - `Downstream` waits until an `mining.authorize` request is entry (RR Q: do we mean Translator?)

5. Translator returns `Ok`
  - `Downstream` sends `Ok`

6. Sv1 Client sends `mining.extranonce-subscribe` + `mining.subscribe`:
  - `Downstream` waits until `mining.extranonce-subscribe` requests are sent

7. Translator sends `server_to_client::Subscribe` messages. The `extranonce1` field will be the
   extranonce prefix (assigned by the Pool to the Translator in (2)) + a unique nonce that the
   Translator assigns to this particular Miner. Translator also sends a `mining.notify` message
   that will derive from the Extended Job and prev hash that the Pool sent to the Translator in (2)
     - `Downstream` creates a unique id to be used as the second part of the extranonce
        e.g. incrementing a counter
     - `Downstream` needs the data about the last prev hash and the last Extended Job from
        `Upstream` that can be done using a global mutex or using a channel or whatever the
         preferred method
     -  `Downstream` creates the above Sv1 messages and sends them to the Miner

8. Sv1 Client sends `mining.submit` as soon as a share is found

9. Translator transforms the Sv1 `mining.submit` message into a `SubmitSharesExtended` Sv2 message
   and send it to the Pool:
     - `Downstream` listens for the message from the Sv1 Client. If one of the message is a 
        `mining.submit`, it transforms it into an Sv2 message
     - `Downstream` sends the Sv2 message to `Upstream` (likely via an async channel)
     - `Upstream` sends the message to the Pool

10. Pool sends a Success Sv2 message to the Translator

11. Translator sends an Sv1 Success message to the Sv1 Client
  

## Some code examples
1. Translator opens an Extended Channel with the Pool with the require min extranonce:
   - `Upstream` opens a TCP connection with the Pool
      * https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/upstream_mining.rs#L199-L203
   - `Upstream` creates an `OpenExtendedChannelRequest` and sends it to the Pool
      * https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/upstream_mining.rs#L40

2. Pool sends `OpenChannelSuccess` + `ExtendedJob` + prev hash:
   - `Upstream` blocks until Pool sends:
     - `OpenExtendedChannelRequest.Success`
     - `NewExtendedMiningJob`
     - `SetNewPrevHash`
       * https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/upstream_mining.rs#L231
       * https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/upstream_mining.rs#L173

3. Translator starts listening for downstream connections
  - As soon as TCP connection is open on the listening address and the Translator instantiates a
    new `Downstream` 
      * https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/main.rs#L166-L171
      * https://github.com/stratum-mining/stratum/blob/main/roles/v2/mining-proxy/src/lib/downstream_mining.rs#L317-L350

4. Sv1 Client sends Sv1 `mining.authorize`
  - `Downstream` waits to receive this message

5. Translator returns an `Ok`
  - `Downstream` sends an `Ok`
    * https://github.com/stratum-mining/stratum/blob/main/examples/sv1-client-and-server/src/main.rs#L183

6. Sv1 Client sends `mining.extranonce-subscribe` + `mining.subscribe`:
  - `Downstream` waits to receive these messages

7. Translator sends `server_to_client::Subscribe` with the `extranonce1` field set to the extranonce
   prefix (assigned by the Pool to the Translator in (2)) + a unique nonce that the Translator
   assigns to this particular Miner. Translator also sends a `Notify` message that will
   derive from the Extended Job and prev hash that the Pool sent to the Translator in (2):
     - `Downstream` creates an unique id to be used as a second part of the extranonce
        e.g. incrementing a counter
     - `Downstream` needs to get the data about the last prev hash and las Extended Job from the
        `Upstream` which can be done using a global mutex or channel or whatever preferred method
          * https://github.com/stratum-mining/stratum/blob/main/roles/v2/pool/src/lib/template_receiver/message_handler.rs#L12-L31
          * https://github.com/stratum-mining/stratum/blob/main/roles/v2/pool/src/lib/template_receiver/mod.rs#L78
          * https://github.com/stratum-mining/stratum/blob/main/roles/v2/pool/src/lib/template_receiver/mod.rs#L231
          * https://github.com/stratum-mining/stratum/blob/main/roles/v2/pool/src/lib/mining_pool/mod.rs#L616-L642
     - `Downstream` creates the above Sv1 messages and sends them to the Sv1 Client

8. Sv1 client sends `mining.submit` as soon as a share is found
9. Translator transforms the Sv1 `mining.submit` into a `SubmitSharesExtended` and sends it to the
   Pool
     - `Downstream` listens for the message from the Sv1 Client. If one of the messages is
       `mining.submit`, it transforms it into a Sv2 message
     - `Downstream` sends the message to the `Upstream` (likely via an async channel)
     - `Upstream` sends the message to the Pool

10. Pool sends a Sv2 success message to the Translator

11. Translator sends Sv1 success message to the Sv1 Client
