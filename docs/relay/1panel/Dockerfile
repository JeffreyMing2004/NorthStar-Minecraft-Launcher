FROM rust:1.99.1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY source/Cargo.toml source/Cargo.lock source/build.rs ./
COPY source/src ./src
COPY source/assets ./assets
# Keep this a debug build; omit the launcher's GUI dependencies.
ENV CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2
RUN cargo build --locked --no-default-features --bin nscl-relay

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 nscl && useradd --uid 10001 --gid nscl --no-create-home nscl
COPY --from=build /build/target/debug/nscl-relay /usr/local/bin/nscl-relay
USER 10001:10001
WORKDIR /data
ENTRYPOINT ["nscl-relay"]
CMD ["serve", "/data/members.json", "127.0.0.1:17630", "/data/frpc-nodes.json"]
