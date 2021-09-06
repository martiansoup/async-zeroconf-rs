# async-zeroconf

`async-zeroconf` is a crate to register ZeroConf services and provides a way of
keeping the service alive using [Tokio] rather than a synchronous event loop.

## Examples

### Publishing a service

```rust
#[tokio::main]
async fn main() -> Result<(), async_zeroconf::ZeroconfError> {
    // Create a service description
    let service = async_zeroconf::Service::new("Server", "_http._tcp", 80);
    // Publish the service
    let service_ref = service.publish().await?;
    // Service kept alive until service_ref dropped
    Ok(())
}
```

### Browsing for services

```rust
#[tokio::main]
async fn main() -> Result<(), async_zeroconf::ZeroconfError> {
    let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    let mut services = browser
        .timeout(tokio::time::Duration::from_secs(2))
        .browse()?;

    while let Some(Ok(v)) = services.recv().await {
        println!("Service = {}", v);
    }
    Ok(())
}
```

### Resolving a service

```rust
#[tokio::main]
async fn main() -> Result<(), async_zeroconf::ZeroconfError> {
    let mut browser = async_zeroconf::ServiceBrowserBuilder::new("_http._tcp");
    let mut services = browser
        .timeout(tokio::time::Duration::from_secs(2))
        .browse()?;

    while let Some(Ok(v)) = services.recv().await {
        let resolved_service = async_zeroconf::ServiceResolver::r(&v).await?;
        println!("Service = {}", resolved_service);
    }
    Ok(())
}
```

## Changelog

- 0.2.0
    - Fix issues with errors on publishing a service
    - `publish` is now an async function as it waits for errors
- 0.1.0
    - Initial version

## License

`async-zeroconf` can be licensed under the MIT license or the Apache 2.0 license.

[Tokio]: https://tokio.rs/
