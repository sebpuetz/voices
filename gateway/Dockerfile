FROM debian:buster-slim
WORKDIR /app
# FIXME: libpq5 needs to be installed because the channels registry is built into the image
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates libpq5 \
    # Clean up
    && apt-get autoremove -y && apt-get clean -y && rm -rf /var/lib/apt/lists/*
COPY --from=voices:build /app/target/release/voices-gateway voices-gateway
CMD ["./voices-gateway"]
