#!/usr/bin/env bash
set -e

DB=media/VSA_shaders.db

if [ ! -f "$DB" ]; then
  echo "BUILD: shader DB not found, building $DB..."
  mkdir -p media
  node VSA_shaders/VSA_loader_tool.mjs
else
  echo "BUILD: shader DB exists ($DB)"
fi

cargo build
