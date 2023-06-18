#!/usr/bin/env bash

set -eux
SCRIPT=$(readlink -f "$0")
# Absolute path this script is in, thus /home/user/bin
SCRIPTPATH=$(dirname "$SCRIPT")
pushd ${SCRIPTPATH}/../
docker build -t voices:build . -f docker/build.Dockerfile
docker build -t voices:runtime . -f docker/runtime.Dockerfile
docker build --no-cache -t voices/gateway:latest . -f gateway/Dockerfile
docker build --no-cache -t voices/channels:latest . -f channels/grpc/Dockerfile
docker build --no-cache -t voices/server:latest . -f voice/grpc/Dockerfile
popd