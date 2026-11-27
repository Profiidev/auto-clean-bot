ARG TARGETARCH
# If TARGETARCH is amd64, result is x86_64. If arm64, result is aarch64.
ARG RUST_ARCH=${TARGETARCH/amd64/x86_64}
ARG RUST_ARCH=${RUST_ARCH/arm64/aarch64}
ARG TARGET=${RUST_ARCH}-unknown-linux-gnu
ARG RUSTFLAGS="-C target-feature=+crt-static"

FROM ghcr.io/profiidev/images/rust-gnu-builder:main@sha256:5e71dc6dc6f1263f670e506e5a1231d6069acce53c9859b98d9dc3fe2e8d4f2c AS backend-planner

ARG TARGET
ARG RUSTFLAGS

COPY bot/Cargo.toml bot/
COPY bot/entity/Cargo.toml bot/entity/
COPY bot/migration/Cargo.toml bot/migration/
COPY ./Cargo.lock ./Cargo.toml ./

RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/app/target \
  cargo chef prepare --recipe-path recipe.json --bin auto-clean-bot

FROM ghcr.io/profiidev/images/rust-gnu-builder:main@sha256:5e71dc6dc6f1263f670e506e5a1231d6069acce53c9859b98d9dc3fe2e8d4f2c AS backend-builder

ARG TARGET
ARG RUSTFLAGS

COPY --from=backend-planner /app/recipe.json .

RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/app/target \
  cargo chef cook --release --target $TARGET

COPY bot/Cargo.toml bot/
COPY bot/src bot/src
COPY bot/entity/Cargo.toml bot/entity/
COPY bot/entity/src bot/entity/src
COPY bot/migration/Cargo.toml bot/migration/
COPY bot/migration/src bot/migration/src
COPY ./Cargo.lock ./Cargo.toml ./

RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/app/target \
  cd bot && cargo build --release --target $TARGET --bin auto-clean-bot \
  && mv ../target/$TARGET/release/auto-clean-bot ../app

FROM scratch

COPY --from=backend-builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=backend-builder /app/app /usr/local/bin/auto-clean-bot

USER 65532:65532

ENTRYPOINT ["auto-clean-bot"]
