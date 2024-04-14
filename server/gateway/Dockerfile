FROM voices:runtime
WORKDIR /app
COPY --from=voices:build /app/target/release/voices-gateway /usr/bin/voices-gateway
ENTRYPOINT ["/usr/bin/voices-gateway"]
