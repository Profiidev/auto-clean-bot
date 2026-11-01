ARG TARGET=x86_64-unknown-linux-musl

FROM ghcr.io/profiidev/images/rust-musl-builder:main@sha256:b89754fac7aa225332f18392001c6dee411da27b40f1a17d62591e9769500373 AS planner

COPY bot/entity/Cargo.toml ./entity/Cargo.toml
COPY bot/migration/Cargo.toml ./migration/Cargo.toml
COPY bot/Cargo.toml Cargo.lock ./

RUN echo "[workspace]" >> Cargo.toml && \
    echo "members = [\"entity\", \"migration\"]" >> Cargo.toml

RUN cargo chef prepare --recipe-path recipe.json --bin auto-clean-bot

FROM ghcr.io/profiidev/images/rust-musl-builder:main@sha256:b89754fac7aa225332f18392001c6dee411da27b40f1a17d62591e9769500373 AS builder

ARG TARGET
ENV TARGET=$TARGET

COPY --from=planner /app/recipe.json .

RUN cargo chef cook --release --target $TARGET

COPY bot/src ./src
COPY bot/entity ./entity
COPY bot/migration ./migration
COPY bot/Cargo.toml Cargo.lock ./

RUN cargo build --release --target $TARGET --bin auto-clean-bot
RUN mv ./target/$TARGET/release/auto-clean-bot ./app

FROM alpine@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a

RUN addgroup -S bot && adduser -S -G bot bot

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

WORKDIR /app
COPY --from=builder /app/app /usr/local/bin/

USER bot

CMD ["app"]
