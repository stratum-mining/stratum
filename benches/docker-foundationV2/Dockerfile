FROM node:14

# Install dependencies
RUN apt-get update && \
    apt-get install -y build-essential libsodium-dev libboost-system-dev

# Clone foundation-v2-server repository
RUN git clone https://github.com/blinkhash/foundation-v2-server.git foundation-server

# Install foundation-v2-server dependencies
WORKDIR /foundation-server
RUN npm install

#copy config file
WORKDIR /docker-foundationV2
COPY config.js /foundation-server/configs/main/config.js

#Add templates
WORKDIR /foundation-server
RUN npm install --save foundation-v2-bitcoin

#config template
WORKDIR /docker-foundationV2
COPY bitcoin.js configs/pools/bitcoin.js

# Expose ports
EXPOSE 3002

# Start foundation-v2-server
WORKDIR /foundation-server
CMD ["npm", "run", "start"]
