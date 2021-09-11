ARG RUST_IMAGE=docker.io/library/rust:1.55.0-bullseye
ARG RUNTIME_IMAGE=docker.io/library/debian:bullseye-slim

FROM $RUST_IMAGE as build
ARG TARGETARCH
WORKDIR /usr/src/ort
COPY . .
RUN --mount=type=cache,target=target \
    --mount=type=cache,from=rust:1.55.0-buster,source=/usr/local/cargo,target=/usr/local/cargo \
    target=$(rustup show | sed -n 's/^Default host: \(.*\)/\1/p') ; \
    cargo build --locked --release --target=$target && \
    mv target/${target}/release/ort /tmp

FROM $RUNTIME_IMAGE
COPY --from=build /tmp/ort /ort
ENTRYPOINT ["/ort"]
