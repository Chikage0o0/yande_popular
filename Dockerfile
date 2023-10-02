FROM rust:1.72.1-alpine3.18 as builder
COPY . /app
WORKDIR /app
RUN apk add --no-cache --virtual .build-deps \
        make \
        musl-dev \
        openssl-dev \
        perl \
        pkgconfig \
    && cargo build --release

FROM alpine:3.18
LABEL maintainer="Chikage <chikage@939.me>" \
      org.opencontainers.image.source="https://github.com/Chikage0o0/yande_popular" \
      org.opencontainers.image.description="Automatically download the most popular images from yande.re and send to vocechat"
COPY --from=builder /app/target/release/yande_popular \
                    /usr/local/bin/yande_popular

RUN mkdir /yande_popular && chown nobody:nobody /yande_popular
USER nobody
VOLUME ["/yande_popular"]
ENV DATA_DIR=/yande_popular
ENTRYPOINT ["/usr/local/bin/yande_popular"]