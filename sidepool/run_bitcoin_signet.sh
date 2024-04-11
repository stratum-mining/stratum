#!/bin/bash

# Define repository and directory variables
REPO_URL="https://github.com/rsantacroce/bitcoin_signet.git"
DIR_NAME="bitcoin_signet"

# Step 1: Clone the repository if it doesn't exist
if [ ! -d "$DIR_NAME" ]; then
    echo "Cloning the bitcoin-signet repository..."
    git clone $REPO_URL
else
    echo "Directory $DIR_NAME already exists; pulling latest changes..."
    cd $DIR_NAME
    git pull origin master
    cd ..
fi

# Step 2: Build the Docker image
echo "Building the Docker image..."
docker build -t bitcoin-signet -f $DIR_NAME/Dockerfile $DIR_NAME

# Step 3: Remove any old container
echo "Removing any existing bitcoin-signet containers..."
docker rm -f bitcoin-signet-instance

# Step 4: Run the Docker container
echo "Running bitcoin-signet container..."
docker run --name bitcoin-signet-instance -d \
    -p 8442:8442 \
    -p 28332:28332 \
    -p 28333:28333 \
    -p 28334:28334 \
    -p 38332:38332 \
    -p 38333:38333 \
    -p 38334:38334 \
    bitcoin-signet

echo "bitcoin-signet is now up and running!"
