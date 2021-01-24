# syntax=docker/dockerfile:experimental

ARG RUST_IMAGE=rust:1.49.0
ARG RUNTIME_IMAGE=scratch

FROM $RUST_IMAGE as build
ARG TARGET=x86_64-unknown-linux-musl
WORKDIR /usr/src/ort
RUN rustup target add $TARGET
COPY . .
RUN --mount=type=cache,target=target \
    --mount=type=cache,from=rust:1.49.0,source=/usr/local/cargo,target=/usr/local/cargo \
    cargo build --locked --release --target=$TARGET && \
    mv target/$TARGET/release/ort /tmp

FROM $RUST_IMAGE as await
ARG LINKERD_AWAIT_VERSION=v0.2.0
WORKDIR /tmp
RUN curl -vsLO https://github.com/olix0r/linkerd-await/releases/download/release/${LINKERD_AWAIT_VERSION}/linkerd-await && chmod +x linkerd-await

FROM $RUNTIME_IMAGE
COPY --from=await /tmp/linkerd-await /linkerd-await
COPY --from=build /tmp/ort /ort
ENTRYPOINT ["/linkerd-await", "--", "/ort"]
