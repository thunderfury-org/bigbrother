FROM rust:1.83.0 as build-env
WORKDIR /app
COPY . /app
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build-env /app/target/release/bigbrother ./
VOLUME ["/app/data"]
CMD ["./bigbrother", "server"]
