# syntax=docker/dockerfile:experimental

ARG RUST_IMAGE=rust:1.50.0
ARG RUNTIME_IMAGE=scratch

FROM $RUST_IMAGE as build
ARG TARGETARCH
WORKDIR /usr/src/ort
COPY . .
RUN --mount=type=cache,target=target \
    --mount=type=cache,from=rust:1.50.0,source=/usr/local/cargo,target=/usr/local/cargo \
    target=$(rustup show | sed -n 's/^Default host: \(.*\)/\1/p' | sed 's/-gnu$/-musl/') ; \
    rustup target add ${target} && \
    cargo build --locked --release --target=$target && \
    mv target/${target}/release/ort /tmp

FROM docker.io/curlimages/curl:7.75.0 as await
ARG LINKERD_AWAIT_VERSION=v0.2.1
ARG TARGETARCH
RUN curl -fvsLo /tmp/linkerd-await https://github.com/olix0r/linkerd-await/releases/download/release/${LINKERD_AWAIT_VERSION}/linkerd-await-${LINKERD_AWAIT_VERSION}-${TARGETARCH} && chmod +x /tmp/linkerd-await

FROM $RUNTIME_IMAGE
COPY --from=await /tmp/linkerd-await /linkerd-await
COPY --from=build /tmp/ort /ort
ENTRYPOINT ["/linkerd-await", "--", "/ort"]
