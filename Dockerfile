# Stage 1: Build the binary
FROM clux/muslrust:stable AS builder

WORKDIR /volume

# Copy all files to the container
COPY . .

# Build the server binary
# We use --release and target x86_64-unknown-linux-musl which is default for this image
RUN cargo build --bin neutun_server --release

# Stage 2: Create the runtime image
FROM alpine:latest

# Copy the static binary from the builder stage
COPY --from=builder /volume/target/x86_64-unknown-linux-musl/release/neutun_server /neutun_server

# client svc
EXPOSE 8080
# ctrl svc
EXPOSE 5000
# net svc
EXPOSE 10002

ENTRYPOINT ["/neutun_server"]
