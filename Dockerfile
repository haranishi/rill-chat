FROM rust:1-bookworm@sha256:ade39a710ef85f3506cffcfec3e8a24ba090badef1dff2fb798f71d878833fea AS builder
WORKDIR /build
RUN rustup target add wasm32-unknown-unknown
COPY . .
RUN cargo build --locked --release --bin rill-chat
RUN mkdir -p /notices/crates /notices/rust \
    && cd "$CARGO_HOME/registry/src" \
    && find . -type f \( -iname 'license*' -o -iname 'licence*' -o -iname 'copying*' -o -iname 'notice*' -o -iname 'copyright*' \) \
         -exec cp --parents -t /notices/crates '{}' + \
    && cd "$(rustc --print sysroot)/share/doc/rust" \
    && for entry in licenses COPYRIGHT-library.html COPYRIGHT LICENSE*; do \
         if [ -e "$entry" ]; then cp -R "$entry" /notices/rust/; fi; \
       done

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 -m 0700 /data
COPY --from=builder /build/target/release/rill-chat /usr/local/bin/rill-chat
COPY LICENSE /usr/share/doc/rill-chat/LICENSE
COPY --from=builder /notices /usr/share/doc/rill-chat/third-party
ENV RILL_BIND=0.0.0.0:3847 RILL_DB=/data/rill.sqlite3 RILL_ENV=production
USER 10001:10001
EXPOSE 3847
HEALTHCHECK --interval=20s --timeout=3s --start-period=5s --retries=3 \
    CMD curl --fail --silent --output /dev/null http://127.0.0.1:3847/healthz
ENTRYPOINT ["/usr/local/bin/rill-chat"]
