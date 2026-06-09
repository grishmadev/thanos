# Thanos

An L4 Load Balancer in the making.

## Architecture

### Proxy Method

Thanos has mainly 2 methods of proxying incoming requests.

- Normal Proxy:
  - Opens a separate TCP connection to Server
  - Server gets Thanos' IP instead of Clients.
  - Good for anonymous browsing

- TProxy (Transparent Proxying):
  - Creates a Transparent Socket connection with Client's IP
  - Server receives Client's IP as normal.
  - Useful when Server needs Clients' IP for business logic.
  - Faster

### Memory Allocator

Thanos uses [MiMalloc](https://github.com/microsoft/mimalloc) for its memory allocator for faster operations.

## Features

No unique features yet.
