FROM --platform=$BUILDPLATFORM tonistiigi/xx:1.7.0@sha256:010d4b66aed389848b0694f91c7aaee9df59a6f20be7f5d12e53663a37bd14e2 AS xx
FROM --platform=$BUILDPLATFORM rust:1.95.0-alpine3.23@sha256:606fd313a0f49743ee2a7bd49a0914bab7deedb12791f3a846a34a4711db7ed2 AS build

COPY --from=xx / /

RUN apk add --no-cache clang lld
COPY . /app
WORKDIR /app
RUN --mount=type=cache,target=/root/.cargo/git/db \
    --mount=type=cache,target=/root/.cargo/registry/cache \
    --mount=type=cache,target=/root/.cargo/registry/index \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    cargo fetch

ARG TARGETPLATFORM
ARG TYPST_AGENT_COMMIT_SHA

RUN xx-apk add --no-cache musl-dev openssl-dev openssl-libs-static
RUN --mount=type=cache,target=/root/.cargo/git/db \
    --mount=type=cache,target=/root/.cargo/registry/cache \
    --mount=type=cache,target=/root/.cargo/registry/index \
    OPENSSL_NO_PKG_CONFIG=1 OPENSSL_STATIC=1 \
    TYPST_AGENT_COMMIT_SHA="$TYPST_AGENT_COMMIT_SHA" \
    OPENSSL_DIR=$(xx-info is-cross && echo /$(xx-info)/usr/ || echo /usr) \
    xx-cargo build --locked -p typst-cli --release --bin typst-agent \
      --features self-update,vendor-openssl && \
    cp target/$(xx-cargo --print-target-triple)/release/typst-agent target/release/typst-agent && \
    xx-verify target/release/typst-agent

FROM alpine:3.23.3@sha256:25109184c71bdad752c8312a8623239686a9a2071e8825f20acb8f2198c3f659
ARG CREATED
ARG REVISION

# Create a non-root user that can be activated with `--user typst`
RUN addgroup -g 1000 typst && \
    adduser -D -u 1000 -G typst typst

LABEL org.opencontainers.image.authors="Anartha Corp Typst Agent maintainers"
LABEL org.opencontainers.image.created=${CREATED}
LABEL org.opencontainers.image.description="Unofficial AI-assisted Typst downstream compiler"
LABEL org.opencontainers.image.documentation="https://github.com/anartha-corp/typst-agent"
LABEL org.opencontainers.image.licenses="Apache-2.0"
LABEL org.opencontainers.image.revision=${REVISION}
LABEL org.opencontainers.image.source="https://github.com/anartha-corp/typst-agent"
LABEL org.opencontainers.image.title="Typst Agent Docker image"
LABEL org.opencontainers.image.url="https://github.com/anartha-corp/typst-agent"
LABEL org.opencontainers.image.vendor="Anartha Corp"

COPY --from=build  /app/target/release/typst-agent /bin
ENTRYPOINT [ "/bin/typst-agent" ]
