FROM debian:buster-slim
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt-get autoremove -y && apt-get clean -y && rm -rf /var/lib/apt/lists/*
COPY --from=voices:build /app/target/release/voices-gateway voices-gateway
CMD ["./voices-gateway"]
