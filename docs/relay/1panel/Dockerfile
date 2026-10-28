ARG RUST_IMAGE=rust:1.99.1-slim-bookworm
ARG RUNTIME_IMAGE=debian:bookworm-slim
FROM ${RUST_IMAGE} AS build
ARG DEBIAN_MIRROR=https://mirrors.cloud.tencent.com
ARG CARGO_MIRROR=nju
COPY configure-apt.sh /usr/local/bin/nscl-configure-apt
RUN sed -i 's/\r$//' /usr/local/bin/nscl-configure-apt \
    && sh /usr/local/bin/nscl-configure-apt "$DEBIAN_MIRROR" \
    && apt-get update && apt-get install -y --no-install-recommends build-essential pkg-config libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY source/Cargo.toml source/Cargo.lock source/build.rs ./
COPY source/.cargo ./.cargo
COPY source/src ./src
COPY source/assets ./assets
# Keep this a debug build; omit the launcher's GUI dependencies.
ENV CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2
RUN set -eu; case "$CARGO_MIRROR" in \
        sjtu|nju) set -- --config "source.crates-io.replace-with=\"$CARGO_MIRROR\"" ;; \
        official) rm -f .cargo/config.toml; set -- ;; \
        *) echo "CARGO_MIRROR must be sjtu, nju, or official" >&2; exit 2 ;; \
    esac; cargo "$@" build --locked --no-default-features --bin nscl-relay

FROM ${RUNTIME_IMAGE}
ARG DEBIAN_MIRROR=https://mirrors.cloud.tencent.com
# The slim runtime needs a CA bundle before its first HTTPS APT request.
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY configure-apt.sh /usr/local/bin/nscl-configure-apt
RUN sed -i 's/\r$//' /usr/local/bin/nscl-configure-apt \
    && sh /usr/local/bin/nscl-configure-apt "$DEBIAN_MIRROR" \
    && apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 nscl && useradd --uid 10001 --gid nscl --no-create-home nscl
COPY --from=build /build/target/debug/nscl-relay /usr/local/bin/nscl-relay
USER 10001:10001
WORKDIR /data
ENTRYPOINT ["nscl-relay"]
CMD ["serve-frpc", "/data/frpc-nodes.json", "127.0.0.1:17630"]
