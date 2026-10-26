FROM alpine:3.24.2 AS builder
ARG TARGETPLATFORM
COPY target/aarch64-unknown-linux-musl/release/json-streams-flat /target/aarch64-unknown-linux-musl/release/
COPY target/x86_64-unknown-linux-musl/release/json-streams-flat /target/x86_64-unknown-linux-musl/release/
RUN if [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
    cp /target/aarch64-unknown-linux-musl/release/json-streams-flat /json-streams-flat; \
    elif [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
    cp /target/x86_64-unknown-linux-musl/release/json-streams-flat /json-streams-flat; \
    fi
RUN apk add ca-certificates && update-ca-certificates

FROM scratch
USER 11000:11000
COPY --from=builder /json-streams-flat /app/
COPY --from=builder /etc/ssl/certs /etc/ssl/certs
ENTRYPOINT ["/app/json-streams-flat"]
