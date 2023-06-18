FROM debian:buster-slim
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates libpq5 \
    # Clean up
    && apt-get autoremove -y && apt-get clean -y && rm -rf /var/lib/apt/lists/*
COPY --from=voices:build /app/target/release/voices-channels-grpc voices-channels-grpc
CMD ["./voices-channels-grpc"]
