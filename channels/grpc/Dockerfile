FROM voices:runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends libpq5 \
    && apt-get autoremove -y && apt-get clean -y && rm -rf /var/lib/apt/lists/*
COPY --from=voices:build /app/target/release/voices-channels-grpc /usr/bin/voices-channels-grpc
ENTRYPOINT ["/usr/bin/voices-channels-grpc"]

