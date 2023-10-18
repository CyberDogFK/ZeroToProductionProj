#docker build --tag zero2prod --file Dockerfile .
#docker run -p 8000:8000 zero2prod
#docker run --rm -p 8000:8000 --network my-network zero2prod | bunyan
FROM lukemathwalker/cargo-chef:latest-rust-1.73.0 AS chef
WORKDIR /app
RUN apt update && apt install lld clang -y

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
ENV SQLX_OFFLINE true
RUN cargo build --release --bin zero2prod

FROM debian:bullseye-slim AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove \
    && apt-get clean \
    && rm -rf /var/lib/apt/list/*

COPY --from=builder /app/target/release/zero2prod zero2prod
COPY configuration configuration
ENV APP_ENVIRONMENT production
ENTRYPOINT ["./zero2prod"]
