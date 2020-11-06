# syntax=docker/dockerfile:experimental

ARG RUST_IMAGE=rust:1.47.0
ARG RUNTIME_IMAGE=debian:buster-20200803-slim

FROM $RUST_IMAGE as build
WORKDIR /usr/src/ort
COPY . .
RUN --mount=type=cache,target=target \
    --mount=type=cache,from=rust:1.47.0,source=/usr/local/cargo,target=/usr/local/cargo \
    cargo build --locked --release &&  mv target/release/ort /tmp

FROM $RUST_IMAGE as await
ARG LINKERD_AWAIT_VERSION=v0.1.3
WORKDIR /tmp
RUN curl -vsLO https://github.com/olix0r/linkerd-await/releases/download/release/${LINKERD_AWAIT_VERSION}/linkerd-await && chmod +x linkerd-await

FROM $RUNTIME_IMAGE as runtime
COPY --from=await /tmp/linkerd-await /usr/bin/linkerd-await
COPY --from=build /tmp/ort /usr/bin/ort
ENTRYPOINT ["/usr/bin/linkerd-await", "--", "/usr/bin/ort"]
