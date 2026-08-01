ARG TARGET=x86_64-unknown-linux-musl

FROM ghcr.io/profiidev/images/rust-musl-builder:main@sha256:4d53e8560c33883970325aa263b99935138b49d3e5df3eb5669ea1fc3458a624 AS planner

COPY bot/entity/Cargo.toml ./entity/Cargo.toml
COPY bot/migration/Cargo.toml ./migration/Cargo.toml
COPY bot/Cargo.toml Cargo.lock ./

RUN echo "[workspace]" >> Cargo.toml && \
    echo "members = [\"entity\", \"migration\"]" >> Cargo.toml

RUN cargo chef prepare --recipe-path recipe.json --bin auto-clean-bot

FROM ghcr.io/profiidev/images/rust-musl-builder:main@sha256:4d53e8560c33883970325aa263b99935138b49d3e5df3eb5669ea1fc3458a624 AS builder

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

FROM alpine@sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b

RUN addgroup -S bot && adduser -S -G bot bot

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

WORKDIR /app
COPY --from=builder /app/app /usr/local/bin/

USER bot

CMD ["app"]
