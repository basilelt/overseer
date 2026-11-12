FROM rust:1-bookworm AS build
WORKDIR /src
COPY contract/wire.rs ./contract/wire.rs
WORKDIR /src/api
COPY api/Cargo.toml api/Cargo.lock ./
COPY api/src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home overseer \
 && mkdir /data && chown 10001:10001 /data && chmod 700 /data
COPY --from=build /src/api/target/release/overseer-api /usr/local/bin/overseer-api
USER 10001
ENV OVERSEER_API_ADDR=0.0.0.0:8080 OVERSEER_API_DB=/data/overseer-api.db
EXPOSE 8080
VOLUME /data
ENTRYPOINT ["/usr/local/bin/overseer-api"]
