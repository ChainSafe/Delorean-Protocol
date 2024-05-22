# syntax=docker/dockerfile:1

# Build stage
FROM rust:bookworm as builder

RUN apt update && \
    apt install -y build-essential libssl-dev mesa-opencl-icd ocl-icd-opencl-dev gcc git bzr jq pkg-config curl clang hwloc libhwloc-dev wget ca-certificates gnupg

WORKDIR /app

COPY . .

RUN make build

# Main stage
FROM debian:bookworm-slim

RUN apt update && \
    apt install -y build-essential libssl-dev curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/bin/ipc-cli /usr/local/bin/ipc-cli

ENTRYPOINT ["/usr/local/bin/ipc-cli"]
