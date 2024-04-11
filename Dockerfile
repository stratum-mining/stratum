# Use the official Rust image as the base
FROM rust:1.75.0

# Install Git
RUN apt-get update && apt-get install -y git \
    && rm -rf /var/lib/apt/lists/*  # Clean up to reduce image size

# Clone the repository
# ARG REPO_URL
# RUN git clone https://github.com/rsantacroce/stratum.git /usr/src/app

COPY . /usr/src/app/

# Set the working directory to the cloned repository
WORKDIR /usr/src/app/roles

# Optionally, switch to a different branch if needed
# RUN git switch pool

# Build the Rust project
RUN cargo build --release

# The command to run when the container starts (example: running the built executable)
CMD ["ls"]
