FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev

COPY . /sources
WORKDIR /sources
RUN cargo build --release
RUN mkdir -p /data

FROM scratch
COPY --from=builder /sources/target/release/bin /pastebin
COPY --from=builder /etc/passwd /etc/passwd

EXPOSE 8000
ENTRYPOINT ["/pastebin", "0.0.0.0:8000", "--store-path", "/data/store.json"]
