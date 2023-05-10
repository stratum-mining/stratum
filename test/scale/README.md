# Scale Test

This test simply outputs the time spent sending 1,000,000 SubmitSharesStandard 
through the system. When you start the test you specify -h <num of hops> -e (for encryption). 
The test spawns <num of hops> "proxies" (ports 19000->19000+<num of hops>) which simply decrypt/encrypt each 
SubmitSharesStandard message coming in (if encryption is on). Then it sends 
1,000,000 share messages to the first proxy and then times the whole system to see 
how long it takes for the last proxy to receive all 1M messages. It uses the same
network_helpers that the pool, and proxies use so it should be a good approximation
of the work they do. 

The test is run with the following command:
NOTE: running without `--release` dramatically slows down the test.

```cargo run --release -- -h 4 -e```
This runs the test with 4 hops and encryption on.

```cargo run --release -- -h 4```
This runs the test with 4 hops and encryption off.



