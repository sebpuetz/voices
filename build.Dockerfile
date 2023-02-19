# Adapted from https://github.com/LukeMathWalker/zero-to-production/blob/main/Dockerfile
FROM lukemathwalker/cargo-chef as planner
WORKDIR /app
COPY . .
# Compute a lock-like file for our project
RUN cargo chef prepare  --recipe-path recipe.json

FROM lukemathwalker/cargo-chef as cacher
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

FROM rust:1.66.1 AS builder
WORKDIR /app
# Copy over the cached dependencies
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
COPY --from=cacher /usr/local/bin/protoc /usr/local/bin/protoc
COPY --from=cacher /usr/local/include /usr/local/include
COPY . .
# Build our application, leveraging the cached deps!
RUN cargo build --release --workspace
