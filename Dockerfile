# syntax=docker/dockerfile:experimental

ARG RUST_IMAGE=rust:1.47.0
ARG RUNTIME_IMAGE=debian:buster-20200803-slim

FROM $RUST_IMAGE as build
WORKDIR /usr/src/ort
COPY . .
RUN --mount=type=cache,target=target \
    --mount=type=cache,from=rust:1.47.0,source=/usr/local/cargo,target=/usr/local/cargo \
    cargo build --locked --release &&  mv target/release/ort /tmp

FROM $RUNTIME_IMAGE as runtime
COPY --from=build /tmp/ort /usr/bin/ort
ENTRYPOINT ["/usr/bin/ort"]
