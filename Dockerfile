# Keep this image tag aligned with channel = "1.85.0" in rust-toolchain.toml.
FROM rust:1.85.0-slim-bookworm

ENV CARGO_TERM_COLOR=always \
    RUST_BACKTRACE=1

WORKDIR /workspace

RUN apt-get update \
    && apt-get install --yes --no-install-recommends make ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY . .

# Keep the lint components explicit because the repository toolchain requests a
# minimal profile and the official image does not include them by default.
RUN rustup component add rustfmt clippy --toolchain 1.85.0

# The workspace has no external Rust or system dependencies at this milestone.
RUN cargo test --locked --offline --workspace --all-targets

# Running the container repeats the same portable-core validation.
CMD ["cargo", "test", "--locked", "--offline", "--workspace", "--all-targets"]
