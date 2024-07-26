# Start from a Rust base image
FROM rust:1.79-bullseye as builder

# Create a new empty shell project
RUN USER=root cargo new --bin recent-messages2
WORKDIR /recent-messages2

# Copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml
COPY ./rust-toolchain.toml ./rust-toolchain.toml 

# Copy your source tree
COPY ./src ./src
COPY ./migrations_main ./migrations_main
COPY ./migrations_shard ./migrations_shard

# Build for release.
RUN cargo build --release

# Our second stage, that will be the final image
FROM debian:bullseye-slim
WORKDIR /app

# Install libssl (needed for most applications)
RUN apt-get update && apt-get install -y libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy the build artifact from the builder stage and set the startup command
COPY --from=builder /recent-messages2/target/release/recent-messages2 .
COPY config.toml .

# Create a directory for messages
RUN mkdir /app/messages

# Start the binary
CMD ["./recent-messages2"]
