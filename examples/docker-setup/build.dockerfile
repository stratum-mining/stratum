# Use a rust image as the base
FROM rust:latest

# Create a working directory
WORKDIR /app

# Copy your project files to the container
COPY . .

# Build your Rust application
RUN cargo build --release

# Set the entry point
CMD ["./target/release/your_app"]
