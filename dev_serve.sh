#!/usr/bin/env bash

simple-http-server --index web &
cargo watch -i .gitignore -i "web/*" -s "wasm-pack build --target web -d web/dist" &&
kill $!
