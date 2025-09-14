# Use the official Rust image as a base
FROM rust:1.85 as builder

# Set the working directory
WORKDIR /app/quark_bot

# Copy the Cargo.toml files and workspace Cargo.lock
COPY quark_bot/Cargo.toml ./
COPY quark_core/Cargo.toml ../quark_core/
COPY quark_server/Cargo.toml ../quark_server/
COPY quark_consumer/Cargo.toml ../quark_consumer/
COPY Cargo.toml ../

# Copy the source code
COPY quark_bot/src/ ./src/
COPY quark_core/src/ ../quark_core/src/
COPY quark_server/src/ ../quark_server/src/
COPY quark_consumer/src/ ../quark_consumer/src/

# Build the application
RUN cargo build --release --bin quark_bot

# Create a new, smaller image for the final application
FROM debian:bookworm-slim

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Set the working directory
WORKDIR /app/quark_bot

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/quark_bot .

# Copy the config directory
COPY quark_bot/config/ ./config/

# Run the application
CMD ["./quark_bot"] 
