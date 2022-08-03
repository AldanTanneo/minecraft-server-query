# Minecraft Server Query

Rust implementation of the Minecraft server [Query UDP protocol](https://wiki.vg/Query)

## [Documentation](https://aldantanneo.github.io/minecraft-server-query)

## Features

Only the blocking API is included when no features are specified. You can use the `tokio` 
or `async-std` features for an async API using their networking primitives.

## Examples

The `blocking` and `async` versions have the same API, adding a few `async` and 
`.await` here and there :

```rust
use minecraft_server_query::blocking::QueryClient;

let client = QueryClient::new("127.0.0.1:25565")?;
let client2 = QueryClient::new_with_port("127.0.0.1", 25565)?;
let client3 = QueryClient::new_with_socket_address(
    "127.0.0.1",
    25565,
    (Ipv4Addr::UNSPECIFIED, 0),
    Some(Duration::from_secs(3)),
)?;

let token = client.handshake()?;
let basic_stat = client.basic_stat(token)?;
let full_stat = client.full_stat(token)?;
```

The convenience function `query` is also available in each module,
and handles the handshake for you:

```rust
let full_stat = minecraft_server_query::blocking::query("127.0.0.1:25565")?;
```
