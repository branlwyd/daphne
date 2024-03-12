##
## BUILD
##
FROM rust:1.76-bookworm AS builder
WORKDIR /build
RUN apt update && \
    apt install -y \
    clang \
    make \
    npm \
    capnproto \
    musl-dev \
    musl-tools

RUN npm install -g wrangler@2.19.0

# Pre-install worker-build and Rust's wasm32 target to speed up our custom build command
RUN cargo install --git https://github.com/cloudflare/workers-rs
RUN rustup target add wasm32-unknown-unknown
RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml Cargo.lock ./
COPY daphne ./daphne
COPY daphne_server ./daphne_server
COPY daphne_service_utils ./daphne_service_utils
COPY daphne_worker ./daphne_worker
COPY daphne_worker_test ./daphne_worker_test

# Build storage proxy.
WORKDIR /build/daphne_worker_test
RUN wrangler publish --dry-run -c wrangler.storage_proxy.toml

# Build service.
WORKDIR /build
RUN cargo build --example service --features test-utils --release --target x86_64-unknown-linux-musl


##
## CONTAINER
##
FROM alpine:3.16 AS test
# FROM debian:bookworm AS test # XXX

# RUN apt update && apt install -y npm bash
RUN apk add --update npm bash

RUN npm install -g miniflare@2.14.0
COPY --from=builder /build/daphne_worker_test/wrangler.storage_proxy.toml /wrangler.toml
COPY --from=builder /build/daphne_worker_test/build/worker/* /build/worker/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/examples/service /service
COPY --from=builder /build/daphne_server/examples/configuration-helper.toml /configuration-helper.toml
COPY wrapper_script.sh /wrapper_script.sh

EXPOSE 8788
ENTRYPOINT ["/bin/bash", "/wrapper_script.sh"]
