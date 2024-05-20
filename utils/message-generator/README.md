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
3. `% cargo run test.json`
4. If the test is in the `/test/message-geneator` directory, you have to lauch it from the MG
   directory using relative path. For example, 
```
cargo run ../../test/message-generator/test/pool-sri-test-1-standard.json
```

## Test execution

The message generator executes a test with the following steps: 
1. Setup Commands: Executes shell commands or bash scripts to be run on start up, e.g. a `bitcoind` a node.
2. Role Connection: Setups up one or two TCP connections, both plain and noise are supported, e.g. a connection to an Upstream role.
3. Execution Commands: Executes shell commands or bash scripts to be run after a connection has been opened between two roles, e.g. a mocked Pool to a test Proxy.
4. Actions: Executes the actions using the previously opened connection. If a SV2 test is executing, each action
   sends zero or more `Sv2Frames` and checks if the received frame satisfies the conditions (defined
   in the action). If a SV1 test is executing, each action sends one or more JSON-RPC Requests and checks if the received Response satisfies the conditions. More than one condition can be defined if more than one message is expected to be
   received.
5. Cleanup Commands: Executes shell commands or bash scripts to be run on test completion, e.g. remove a `bitcoind` `datadir`.

## True tests, mocks and modules
The true tests are located in `test/message-generator/test`. Some files have the structure of a 
test but in fact they are not. 
For example, the file in 
`test/message-generator/mock` are mocks of application, namely they are tests whose purpose is 
to pretend to be an SV2 role. Currently only the TemplateProvider and the JobDeclarator are the 
only roles mocked. These mocks are usually used in the true tests. For example, in the test
`test/message-generator/test/pool-sri-test-standard-1.json`
the pool has a mocked environment, i.e. the JD and TP mocks.
The files in `test/message-generator/messages` also are not true tests, but the are intended to be
modules. For example, the `common_messages.json` is the module that contains the frame builders of
the common messages for the true tests and the mocks.

True tests are made to be run and produce positive outcome. 
Mocks and common messages are meant to work as a part of a true test and they are not supposed to
be run as standalone.


## Test format

Tests are written in json and must follow the below format:

### version

`version` is a string that indicates if the test is a SV1 test or a SV2 test. It can be either `"1"` or `"2"`.

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

`common_messages` is an array of messages (defined below) belonging to the common (sub)protocol, 
where the common subprotocol is composed by: `SetupConnection` and `SetupConnectionSuccees` and `SetupConnectionError`.
This field is optional.

### mining_messages

`mining_messages` is an array of messages (defined below) belonging to the mining (sub)protocol. This field is optional.

### job_declaration_messages

`job_declaration_messages` is an array of messages (defined below) that belongs to the job declaration (sub)protocol. This field is optional.

### template_distribution_messages

`template_distribution_messages` is an array of messages (defined below) that belongs to the template distribution (sub)protocol. This field is optional.

### sv1_messages

`sv1_messages` is an array of messages that belongs to the SV1 protocol. If `version` is `"2"` this field should be empty.

**Definition of messages mentioned above**

A message is an object with two field `message` and `id`.
The `message` field contains the actual message. The `id` field contains a unique id
used to refer to the message in other part of the test.
For SV2 messages, the `message` field is an object composed by:
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

For SV1 messages, the `message` field is a JSON-RPC StandardRequest, composed by the fields `id`, `method` and `params`.


```json
{
    "sv1_messages": [
        {
            "message": {
                "id": 1,
                "method": "mining.subscribe",
                "params": ["cpuminer"]
            },
            "id": "mining.subscribe"
        },
        {
            "message": {
                "id": 2,
                "method": "mining.authorize",
                "params": ["username", "password"]
            },
            "id": "mining.authorize"
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
If the frame relative to common messages is defined is a different file (for example, some 
common_messages frames are defined in `/test/message-geneator/messages/common_messages.json`), to 
use it you have to use the syntax `<address::id>`. For example, in the test 
`/test/message/generator/test`, the following message 
```
   {
       "type": "automatic",
       "message_id": "test/message-generator/messages/common_messages.json::setup_connection_success_template_distribution"
   }
```
calls the id `setup_connection_success_template_distribution` that appears in the file 
`test/message-generator/messages/common_messages.json`. In the main file, the id of this
message will be the abbreviated with `setup_connection_success_template_distribution`. 
 


### actions

`actions` is an array of objects. If the test version is "2", each object is composed by:
1. `role`: can be either `client` or `server`. This because the message generator can act at the
   same time as a client and as a server for example when we are mocking a proxy.
2. `messages_ids`: an array of strings, that are ids of messages previously defined.
3. `results`: is an array of objects, used by the message generator to test if certain property of
   the received frames are true or not.  Accepts values:
    - "type": String - match option - match_message_type, match_message_field, match_message_len, or match_extension_type
    - "value": Array - varries depending on "type"

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
                        [
                            "request_id",
                            {"U32": 89}
                        ]
                    ]
                }
            ] 
        }
    ]
}
```

If the test version is "1", each object is composed by:
1. `messages_ids`: an array of strings, that are ids of sv1_messages previously defined.
2. `results`: is an array of objects, used by the message generator to test if certain property of
   the received Responses are true or not.  Accepts values:
    - "type": String - match option - match_message_id, match_message_field
    - "value": Array - varries depending on "type"

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

## Using Message Generator to produce test coverage with llvm-cov

Information on installation and use of llvm-cov found here: https://crates.io/crates/cargo-llvm-cov/0.1.13
More information on underlying dependancies here: https://doc.rust-lang.org/rustc/instrument-coverage.html

### Including code coverage in command

Example of llvm-cov code coverage run with pool setup_command
llvm-cov requires a minimum of rustc 1.60.0 and LLVM 13.0.0

```json
        {
            "command": "cargo",
            "args": [
                        "llvm-cov",
                        "--no-report",
                        "run",
                        "-p",
                        "pool",
                        "--",
                        "-c",
                        "./roles/v2/pool/pool-config.toml"
            ],
            "conditions": {...} 
        },
```

`"--no-report"` caches the results in the background until the end of the test.  It also allows you collect coverage data from other processes in parallel by adding `"llv-cov --no-report"`, and all report data is reflected in the final report. 

To generate the report, execute the `llvm-cov report` command in the cleanup_commands.  This example adds an output file path(from project root), the output file name and type, and paths to ignore in the report.

```json
    "cleanup_commands": [
        {
            "command": "cargo",
            "args": [
                        "llvm-cov",
                        "--ignore-filename-regex",
                        "utils/|experimental/|protocols/",
                        "--output-path",
                        "target/tmp/pool_translator_cov.txt",
                        "report"
            ],
            "conditions": "None"
        }
    ]
```
