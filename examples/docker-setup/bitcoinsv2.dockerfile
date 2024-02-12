# Use an official Alpine Linux as the parent image
FROM alpine:latest

# Set environment variables for Bitcoin Core version and installation directory
ENV BITCOIN_DIR=/bitcoin

# Install necessary dependencies
RUN apk --no-cache add build-base autoconf automake libtool boost-dev openssl-dev db-c++ db-dev miniupnpc-dev libevent-dev git

# Download the selected Stratum source code zip file
RUN git clone https://github.com/Sjors/bitcoin.git

# Build Bitcoin Core
WORKDIR $BITCOIN_DIR

RUN git switch 2023/11/sv2-poll

# Run autogen.sh
RUN ./autogen.sh

# Configure
RUN ./configure

# Build with parallel jobs (use "-j N" for N parallel jobs)
RUN make

# Optionally, you can install Bitcoin Core to the system (not recommended for containers)
RUN make install

# Copy the custom bitcoin.conf file into the container
#COPY bitcoin.conf /root/.bitcoin/bitcoin.conf

# Create a volume for blockchain data and configuration files
# docker run -v /path/to/host/directory:/root/.bitcoin bitcoin-sv2
VOLUME ["/root/.bitcoin"]

# Expose Bitcoin P2P and RPC ports (optional)
EXPOSE 8333 8332 8442

# Use the entrypoint to start bitcoind with the custom configuration
ENTRYPOINT ["/bitcoin/src/bitcoind", "-stratumv2", "-stratumv2port=8442", "--regtest"]