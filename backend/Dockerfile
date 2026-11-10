# Build stage: compile the API server (migrations are embedded at compile time).
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
# Pre-build dependencies for better layer caching.
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY migrations ./migrations
COPY assets ./assets
COPY src ./src
RUN touch src/main.rs && cargo build --release

# Runtime stage.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 workhub && mkdir -p /data/blobs && chown workhub /data/blobs
COPY --from=build /src/target/release/workhub-server /usr/local/bin/workhub-server
USER workhub
ENV BIND_ADDR=0.0.0.0:8080 STORAGE_DIR=/data/blobs
EXPOSE 8080
CMD ["workhub-server"]
