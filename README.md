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

This was tested against 3 [Rust Servers](https://github.com/grishmadev/simpleserverrust.git) producing 28k RPS individually.

```sh
# Tested with hey benchmark tool
hey -n 1000000 -c 100 <proxy port>
```

**_Note:_** This was tested with Thanos on "normal" method.

| Proxy     | Strategy          | RPS     | Slowest Response Time | Average Response Time |
| --------- | ----------------- | ------- | --------------------- | --------------------- |
| `Thanos`  | Round Robin       | ~ 20800 | ~ 0.0934 s            | ~ 0.0048 s            |
| `HAProxy` | Round Robin       | ~ 20500 | ~ 0.0770 s            | ~ 0.0050 s            |
| `Envoy`   | Round Robin       | ~ 18000 | ~ 0.0970 s            | ~ 0.0055 s            |
| `Thanos`  | Least Connections | ~ 20900 | ~ 0.1675 s            | ~ 0.0047 s            |
| `HAProxy` | Least Connections | ~ 19500 | ~ 0.0671 s            | ~ 0.0051 s            |
| `Envoy`   | Least Connections | ~ 18000 | ~ 0.0666 s            | ~ 0.0055 s            |

Normal vs TProxy Method

| Method | Strategy          | RPS     | Slowest Response Time | Average Response Time |
| ------ | ----------------- | ------- | --------------------- | --------------------- |
| Normal | Round Robin       | ~ 20800 | ~ 0.0934 s            | ~ 0.0048 s            |
| Normal | Least Connections | ~ 20900 | ~ 0.1675 s            | ~ 0.0047 s            |
| TProxy | Round Robin       | ~ 22200 | ~ 0.0826 s            | ~ 0.0045 s            |
| Tproxy | Least Connections | ~ 22100 | ~ 0.0603 s            | ~ 0.0045 s            |

## How to run

### Config File

Thanos can use configuration from config file located in `$HOME/.config/thanos/thanos.conf` by default.

- Setting up Port:

```
port = 9000; # Make sure all statements end in ';'
```

- Setting Server List:

```
servers = [127.0.0.1:8888, 127.0.0.1:8889, 127.0.0.1:8890];
# or server = ...;
# or server = ["127.0.0.1:8888", ...];
```

- Setting Proxy Method:

```
method = "tproxy"; # For Setting Transparent Proxy
# or
method = "normal"; # For Setting Normal Proxy
```

- Setting Balance Strategy

```
strategy = "roundrobin";
# or
strategy = "leastconnection";
```

- Setting Designated CPU Cores Manually

```
core = 2;
```

- Assigning config from different location

```
thanos -C <config source>
```

### Command Line

```sh
thanos -p 9000 \
-s 127.0.0.1:8888 \
-s 127.0.0.1:8889 \
-s 127.0.0.1:8890 \
-m normal \
-S roundrobin \
-c 2
```

## Features

No unique features yet.
