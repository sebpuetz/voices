#!/usr/bin/env bash

set -eux
SCRIPT=$(readlink -f "$0")
# Absolute path this script is in, thus /home/user/bin
SCRIPTPATH=$(dirname "$SCRIPT")
pushd ${SCRIPTPATH}/../
docker build -t voices:runtime . -f docker/runtime.Dockerfile \
&& docker build -t voices/standalone:latest . -f docker/standalone.Dockerfile
popd