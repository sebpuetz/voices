#!/usr/bin/env bash

set -eux
docker build -t voices:build . -f build.Dockerfile

pushd gateway
docker build --no-cache -t voices/gateway:latest .
popd

pushd channels-internal
docker build --no-cache -t voices/channels:latest .
popd

pushd voice-server
docker build --no-cache -t voices/server:latest .
popd
