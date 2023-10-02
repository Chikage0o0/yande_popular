FROM rust:1.72.1-alpine3.18 as builder
COPY . /app
WORKDIR /app
RUN apk add --no-cache --virtual .build-deps \
        make \
        musl-dev \
        openssl-dev \
        perl \
        pkgconfig \
    && cargo build --release --target x86_64-unknown-linux-musl

FROM gcr.io/distroless/static:nonroot
LABEL maintainer="Chikage <chikage@939.me>" \
      org.opencontainers.image.source="https://github.com/Chikage0o0/yande_popular" \
      org.opencontainers.image.description="Automatically download the most popular images from yande.re and send to vocechat"
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/yande_popular \
                    /usr/local/bin/yande_popular
USER nonroot:nonroot
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/yande_popular","--data_dir","/data"]