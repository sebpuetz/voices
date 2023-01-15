# Standalone

Standalone setup runs the voice server, channels registry and gateway in a single binary. There is an external dependency on Postgres.

## Database setup

```bash
# requires diesel_cli, in turn depends on libpq
cargo install diesel_cli --no-default-features --features=postgres
DATABASE_URL=postgres://postgres:password@localhost:5432/voices_channels diesel database setup
```

## Gateway

```bash
# run all components integrated and listen for WebSockets / HTTP on 33332
# defaults to opening UDP ports 33333 and 33334 for the voice server
# expects postgres postgres://postgres:password@localhost:5432/voices_channels
RUST_LOG=DEBUG,tower=warn,h2=warn voices-gateway --ws-listen-port 33332 standalone
```

# Distributed

Distributed setup runs voice server instances, channel registry instances and gateway instances independently of each other. There is an additional dependency on a Redis instance to broadcast channel events across different gateway instances.

## Gateway

```bash
# listen for WebSockets / HTTP on 33332
# expect channels registry running on http://localhost:33330
RUST_LOG=DEBUG,tower=warn,h2=warn voices-gateway --ws-listen-port 33332 distributed --redis-conn redis://127.0.0.1:6379/
```

## Voice Server

```bash
# expect channels registry running on http://localhost:33330
# registers itself at channels registry with --http-host-url=http://localhost and --http-port=33331
# generates a random server-id at startup used to identify itself at channels registry
# defaults to opening UDP ports 33333 and 33334
RUST_LOG=debug,tower=warn,h2=warn voice-server
```

## Channels Registry

```bash
# listens for HTTP / gRPC on on http://localhost:33330
# expects migrated voices_channels db postgres://postgres:password@localhost:5432/voices_channels
RUST_LOG=debug,tower=warn,h2=warn voices-channels
```

# Dev Deployment

Deploys 3 instances of `channels`, `gateway` and `voice-server` via docker-compose. The gateways
are behind an nginx reverse proxy listening on port `8000`, the `voice-server` instances start
assigning up to 100 UDP ports from `12222`, `22222` and `32222` respectively. The `voice-server`s
are configured to return their local listen address, i.e. they are not reachable from the internet
without modifying the `UDP_HOST` variable in their deployments to something reachable from the
internet.

```bash
./docker-build.sh
# bring up 3 channels, gateway and voice-server instances along nginx, redis and postgres
docker-compose up -d
# to follow the logs
# docker-compose logs -f 
```