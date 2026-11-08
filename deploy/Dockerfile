# One Rust server for every platform. Build on the native builder CPU, cross-link
# ARM binaries, then package the target-platform runtime. No desktop GUI build.
FROM --platform=$BUILDPLATFORM rust:1.89.0-bookworm AS build
ARG TARGETARCH
ARG TARGETVARIANT
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
COPY assets ./assets
RUN set -eu; case "$TARGETARCH" in \
      amd64) target=x86_64-unknown-linux-gnu ;; \
      arm64) target=aarch64-unknown-linux-gnu; apt-get update; apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross ;; \
      arm) test "$TARGETVARIANT" = v7; target=armv7-unknown-linux-gnueabihf; apt-get update; apt-get install -y --no-install-recommends gcc-arm-linux-gnueabihf libc6-dev-armhf-cross ;; \
      *) echo 'Unsupported architecture'; exit 1 ;; esac; \
    rustup target add "$target"; \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    cargo build --locked --release -p amp-cast --target "$target"; \
    mkdir /out; cp target/"$target"/release/amp-cast target/"$target"/release/amp-prepare /out/

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/gitwide-cloud/amp-verse"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.description="AMP Cast headless server candidate; hardware validation required"
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* && mkdir /data /music && chown 1000:1000 /data /music
COPY --from=build /out/ /usr/local/bin/
COPY LICENSE THIRD_PARTY_NOTICES.md /usr/share/doc/amp-verse/
ENV AMP_CAST_BIND=0.0.0.0:8787 AMP_CAST_CONFIG_DIR=/data/cast AMP_DEVICE_CONFIG_DIR=/data/device
EXPOSE 8787
USER 1000:1000
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s CMD curl --fail --silent http://127.0.0.1:8787/health || exit 1
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/amp-cast"]
CMD ["/music"]
