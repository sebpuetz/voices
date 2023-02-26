FROM debian:buster-slim
WORKDIR /app
RUN apt-get update -y \
    # FIXME: remove libpq5 after factoring out channels registry client
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt-get autoremove -y && apt-get clean -y && rm -rf /var/lib/apt/lists/*
COPY --from=voices:build /app/target/release/voice-server voices-server
CMD ["./voices-server"]
