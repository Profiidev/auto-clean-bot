ARG TARGET=x86_64-unknown-linux-musl

FROM ghcr.io/profiidev/images/rust-musl-builder:main AS planner

COPY bot/entity/Cargo.toml ./entity/Cargo.toml
COPY bot/migration/Cargo.toml ./migration/Cargo.toml
COPY bot/Cargo.toml Cargo.lock ./

RUN echo "[workspace]" >> Cargo.toml && \
    echo "members = [\"entity\", \"migration\"]" >> Cargo.toml

RUN cargo chef prepare --recipe-path recipe.json --bin auto-clean-bot

FROM ghcr.io/profiidev/images/rust-musl-builder:main AS builder

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

FROM alpine

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

WORKDIR /app
COPY --from=builder /app/app /usr/local/bin/

CMD ["app"]