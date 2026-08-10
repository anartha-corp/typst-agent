FROM --platform=$BUILDPLATFORM tonistiigi/xx AS xx
FROM --platform=$BUILDPLATFORM rust:alpine AS build

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

RUN xx-apk add --no-cache musl-dev openssl-dev openssl-libs-static
RUN --mount=type=cache,target=/root/.cargo/git/db \
    --mount=type=cache,target=/root/.cargo/registry/cache \
    --mount=type=cache,target=/root/.cargo/registry/index \
    OPENSSL_NO_PKG_CONFIG=1 OPENSSL_STATIC=1 \
    OPENSSL_DIR=$(xx-info is-cross && echo /$(xx-info)/usr/ || echo /usr) \
    xx-cargo build -p typst-cli --release --bin typst-agent && \
    cp target/$(xx-cargo --print-target-triple)/release/typst-agent target/release/typst-agent && \
    xx-verify target/release/typst-agent

FROM alpine:latest
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
