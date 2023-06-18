# Adapted from https://github.com/LukeMathWalker/zero-to-production/blob/main/Dockerfile
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef

FROM chef AS planner
WORKDIR /app
COPY . .
# Compute a lock-like file for our project
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef AS builder
WORKDIR /app
ENV PROTOC_ZIP=protoc-21.12-linux-x86_64.zip
RUN curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v21.12/$PROTOC_ZIP \
    && unzip -o $PROTOC_ZIP -d /usr/local bin/protoc \
    && unzip -o $PROTOC_ZIP -d /usr/local 'include/*' \ 
    && rm -f $PROTOC_ZIP
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates libpq-dev cmake
COPY --from=planner /app/recipe.json recipe.json
# Build our project dependencies, not our application!
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .

# Build our application, leveraging the cached deps!
RUN cargo build --release --workspace --no-default-features --features=distributed
