# TCP Server

Hey! Welcome to this repo :)

This repo contains a simple chat server written in Rust. The server is able to handle multiple clients at the same time and it is able to send messages to all the clients connected to it.

## How to run the server

Note that it runs on port 8888

```bash
RUST_LOG=info cargo run
```

## How to connect to the server

```bash
nc localhost 8888
```

You should receive a `LOGIN` message from the server. You can now start sending messages to the server.

## Decisions

### Single Threaded

Note that while the solution is singlethreaded, it depends on Tokio's scheduler to handle the fact that we do have multiple concurrent operations meaning we need to use Channels. If I were to go for a latency / throughput optimized solution I would probably go for a `mio` based solution and depend on `epoll` more directly.

### Error handling

I'm using `anyhow` for error handling. In a real production setting I would probably opt to go with `thiserror` for more structured error handling.
