# syntax=docker/dockerfile:experimental

ARG RUST_IMAGE=rust:1.47.0
ARG RUNTIME_IMAGE=debian:buster-20200803-slim

FROM $RUST_IMAGE as build
WORKDIR /usr/src/ortiofay
COPY . .
RUN --mount=type=cache,target=target \
    --mount=type=cache,from=rust:1.45.2,source=/usr/local/cargo,target=/usr/local/cargo \
    cargo build --locked --release &&  mv target/release/ortiofay /tmp

FROM $RUNTIME_IMAGE as runtime
COPY --from=build /tmp/ortiofay /usr/bin/ortiofay
ENTRYPOINT ["/usr/bin/ortiofay"]
