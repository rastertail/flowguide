#!/usr/bin/env bash

simple-http-server --index web &
cargo watch -i .gitignore -s "wasm-pack build --target web -d web/dist"
