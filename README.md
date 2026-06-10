# Thanos

![Dialogue](assets/thanos.jpg)

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

### Memory Allocator

Thanos uses [MiMalloc](https://github.com/microsoft/mimalloc) for its memory allocator for faster operations.

## Performance \(Dual Core CPU | Intel Celeron\)

```sh
# Tested with hey benchmark tool
hey -n 1000000 -c 100 <proxy port>
```

| Proxy     | RPS     | Maximum Response Time | Average Response Time |
| --------- | ------- | --------------------- | --------------------- |
| `Thanos`  | ~ 20800 | ~ 0.0934 s            | ~ 0.0048 s            |
| `HAProxy` | ~ 20500 | ~ 0.0770 s            | ~ 0.0050 s            |
| `Envoy`   | ~ 18000 | ~ 0.0970 s            | ~ 0.0055 s            |

## Features

No unique features yet.
