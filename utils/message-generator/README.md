# Message Generator

Little utility to execute interoperability tests between SRI and other Sv2 complaint software.

## Try it
1. Stop any `bitcoind` regtest processes running (the `message-generator` starts it for you).
2. In `test.json` specify `bitcoind`, `bitcoin-cli`, and `datadir` paths:
```
...
    "setup_commands": [
        {
            "command": "PATH TO bitcoind",
            "args": ["--regtest", "--datadir=PATH TO bitcoin datadir"],

...

        {
            "command": "PATH TO bitcoin-cli",
            "args": [
                        "--regtest",
                        "--datadir=PATH TO bitcoin datadir",
...
```
3. `% cargo run ../test.json`

## Test execution

The message generator executes a test with the following steps:
1. Setup Commands: Executes shell commands or bash scripts to be run on start up, e.g. a `bitcoind` a node.
2. Role Connection: Setups up one or two TCP connections, both plain and noise are supported, e.g. a connection to an Upstream role.
3. Execution Commands: Executes shell commands or bash scripts to be run after a connection has been opened between two roles, e.g. a mocked Pool to a test Proxy.
4. Actions: Executes the actions. Using the previously opened connection, each action
   sends zero or more `Sv2Frames` and checks if the received frame satisfies the conditions (defined
   in the action). More than one condition can be defined if more than one message is expected to be
   received.
5. Cleanup Commands: Executes shell commands or bash scripts to be run on test completion, e.g. remove a `bitcoind` `datadir`.

## Test format

Tests are written in json and must follow the below format:

### doc

`doc` is an array of strings that explain what the test do. This field is optional.

```json
{
    "doc": [
        "This test do:",
            "1) launch bitcoind in regtest and wait for TP initialization",
            "2) create a bunch of block using bictoin-cli",
            "..."
    ]
}
```

### common_messages

`common_messages` is an array of messages (defined below) belonging to the common (sub)protocol. This field is optional.
Where the common subprotocol is composed by: `SetupConnection` and `SetupConnectionSuccees` and `SetupConnectionError`.

### mining_messages

"mining_messages` is an array of messages (defined below) belonging to the mining (sub)protocol. This field is optional.

### job_negotiation_messages

`job_negotiation_messages` is an array of messages (defined below) that belongs to the job negotiation (sub)protocol. This field is optional.

### template_distribution_messages

`template_distribution_messages` is an array of messages (defined below) that belongs to the template distribution (sub)protocol. This field is optional.

**Definition of messages mentioned above**

A message is an object with two field `message` and `id`.
The `message` field contains the actual Sv2 message. The `id` field contains a unique id
used to refer to the message in other part of the test.
The `message` field is an object composed by:
1. type: message type name as defined in the Sv2 spec is a string eg `SetupConnection`
2. all the other field of that specific message as defined in the Sv2 spec with the only exception
   of the field `protocol` that is a string and not a number eg instead of `0` we have
   `MiningProtocol`


```json
{
    "common_messages": [
        {
            "message": {
                "type": "SetupConnection",
                "protocol": "MiningProtocol",
                "min_version": 2,
                "max_version": 2,
                "flags": 0,
                "endpoint_host": "",
                "endpoint_port": 0,
                "vendor": "",
                "hardware_version": "",
                "firmware": "",
                "device_id": ""
            },
            "id": "setup_connection"
        }
    ]
}
```

### frame_builders

Objects in `frame_builders` are used by the message generator to construct Sv2 frames in order to send the message
to the tested software. Objects in `frame_builders` can be either **automatic** (where the sv2 frame header is
constructed by the SRI libs and is supposed to be correct) or **manual** (if we want to test a software
against an incorrect frame).

`frame_builders` is an array of objects. Every object in `frame_builders` must contain `message_id`, that is a 
string with the id of the previously defined message. In the example below, the message id refers to
the item `setup_connection` of `common_messages`. Every object in `frames` must have the
field `type`, a string that can be either `automatic` or `manual` with meaning of the paragraph
above.

If `type` == `manual` the object must contain 3 additional fields:
1. `message_type`: a string the must start with `0x` followed by an hex encoded integer not bigger
   than `0xff`
2. `extension_type`: a string composed by 16 elements, each element must be either `0` or `1`, the
   elements can be separated by `_` that is not counted as element so we can have as many
   separator as we want eg: `0000_0000_0000_0000`
   Separator are there only to human readability and are removed when the test is parsed.
3. `channel_msg`: a `bool`

```json
{
    "frame_builders": [
        {
            "type": "automatic",
            "message_id": "setup_connection"
        },
        {
            "type": "manual",
            "message_id": "close_channel",
            "message_type": "0x18",
            "extension_type": "0000_0000_0000_0000",
            "channel_msg": true
        }
    ]
}
```


### actions

`actions` is an array of objects. Each object is composed by:
1. `role`: can be either `client` or `server`. This because the message generator can act at the
   same time as a client and as a server for example when we are mocking a proxy.
2. `messages_ids`: an array of strings, that are ids of messages previously defined.
3. `results`: is an array of objects, used by the message generator to test if certain property of
   the received frames are true or not.

```json
{
    "actions": [
        {
            "message_ids": ["open_standard_mining_channel"],
            "role": "client",
            "results": [
                {
                    "type": "match_message_field",
                    "value": [
                        "MiningProtocol",
                        "OpenStandardMiningChannelSuccess",
                        "request_id",
                        {"U32": 89}
                    ]
                }
            ] 
        }
    ]
}
```

### setup commands

An array of commands (defined below) that are executed before that the message generator open one
ore more connection with the tested software.

### excution commands

An array of commands (defined below) that are executed after that the message generator open one
ore more connection with the tested software.

### cleanup_commands commands

An array of commands (defined below) that are executed after that all the actions have been executed.
TODO this commands should be executed not only if all the action pass but also if something fail

**Definition of commands mentioned above**

A command is an object with the following fields:
1. `command`: the bash command that we want to execute
2. `args`: the command's args
3. `condition`: can be either `WithConditions` or `None` in the first case the command executor
   will wait for some condition before return in the latter it just launch the command and return

`WithConditions` is an object composed by the following fields:
1. `conditions`: an array of object that must be true or false on order for the executor to
   return
2. `timer_secs`: number of seconds after then the command is considered stuck and the test fail
3. `warn_no_panic`: `bool` if true the test do not fail with panic if timer secs terminate but it
   just exit (passing) and emit a warning

The objects contained in conditions are structured in the following way:
1. `output_string`: a string that we need to check in the StdOut or StdErr
2. `output_location`: say if the string is expected in StdOut or StdErr
3. `condition`: a `bool` if true and we have `output_string` in `output_location` the executor return
   and keep going, if false the executor fail.

In the below example we launch `bitcoind` we wait for `"sv2 thread start"` in StdOut and we fail if
anything is written in StdErr
```json
{
    "setup_commands": [
        {
            "command": "./test/bitcoind",
            "args": ["--regtest", "--datadir=./test/bitcoin_data/"],
            "conditions": {
                "WithConditions": {
                    "conditions": [
                        {
                            "output_string": "sv2 thread start",
                            "output_location": "StdOut",
                            "condition": true
                        },
                        {
                            "output_string": "",
                            "output_location": "StdErr",
                            "condition": false
                        }
                    ],
                    "timer_secs": 10,
                    "warn_no_panic": false
                }
            }
        }
    ]
}
```

### role

`role` is a string and can be on of the three: client server proxy
1. If we have client we expect to have a `downstream` field
2. If we have server we expect to have an `upstream` field
3. If we have proxy we expect to have both

### downstream

`downstream` is an object the contain the info need to open a connection as a downstream is composed by:
1. `ip`: a string with the upstream ip address
2. `port`: a string with the port address
3. `pub_key`: optional if present we open a noise connection if no we open a plain connection, is a
   string with the server public key

### upstream

`upstream` is an object the contain the info need to start listening:
1. `ip`: a string with the ip where we will accept connection
2. `port`: a string with the port address
3. `pub_key`: optional if present accept noise connection if no plain connection
3. `secret_key`: optional if present accept noise connection if no plain connection
