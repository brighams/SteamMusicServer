#!/usr/bin/env bash
set -e

mkdir -p media
node VSA_tools/VSA_loader_tool.mjs
cargo build
