FROM ekidd/rust-musl-builder:1.50.0 AS builder

WORKDIR /home/rust/

# Cache deps in separate layer
RUN echo "fn main() {}" > src/main.rs
COPY --chown=rust:rust Cargo.toml Cargo.lock ./
RUN cargo build --release

# Now to build the app
COPY --chown=rust:rust . ./
RUN sudo touch src/main.rs
RUN cargo build --release

FROM alpine:3.12
RUN apk --no-cache add ca-certificates curl

USER 1000
COPY --from=builder --chown=1000:1000 \
   /home/rust/target/x86_64-unknown-linux-musl/release/weather /usr/local/bin/weather

ENV ROCKET_ADDRESS="0.0.0.0"
EXPOSE 8000
HEALTHCHECK CMD curl -f http://localhost:8000 || exit 1

ENTRYPOINT ["/usr/local/bin/weather"]