# syntax=docker/dockerfile:1.7

# Build stage: compiles runcard-server in release mode. The registry and the
# target directory are BuildKit cache mounts, so a code change does not
# rebuild the dependencies.
FROM rust:1.99-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY fonts ./fonts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin runcard-server \
    && cp target/release/runcard-server /runcard-server

# Runtime stage: a distroless image with libc and libgcc only. The fonts and
# the card templates are compiled into the binary, so nothing else is needed.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /runcard-server /runcard-server
ENV RUNCARD_BIND=0.0.0.0:8080
EXPOSE 8080
USER nonroot
ENTRYPOINT ["/runcard-server"]
