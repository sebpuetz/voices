FROM voices:runtime
WORKDIR /app
COPY --from=voices:build /app/target/release/voices-voice-grpc /usr/bin/voices-voice-grpc
ENTRYPOINT ["/usr/bin/voices-voice-grpc"]
