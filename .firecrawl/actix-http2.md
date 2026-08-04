[Skip to main content](https://actix.rs/docs/http2/#__docusaurus_skipToContent_fallback)

[![Actix Web Logo](https://actix.rs/img/logo-icon.png)\\
**Actix Web**](https://actix.rs/) [Documentation](https://actix.rs/docs) [Community](https://actix.rs/community) [Code](https://actix.rs/code)

[Chat on Discord](https://discord.gg/NWpN5mmg3x)[GitHub repository](https://github.com/actix/actix-web)

```

```

- [Introduction](https://actix.rs/docs/http2/#)

- [Basics](https://actix.rs/docs/http2/#)

- [Advanced](https://actix.rs/docs/http2/#)

- [Protocols](https://actix.rs/docs/http2/#)

  - [WebSockets](https://actix.rs/docs/websockets)
  - [HTTP/2](https://actix.rs/docs/http2)
- [Patterns](https://actix.rs/docs/http2/#)

- [Diagrams](https://actix.rs/docs/http2/#)

- [Actix](https://actix.rs/docs/http2/#)

- [API Documentation](https://actix.rs/docs/http2/#)


- [Home page](https://actix.rs/)
- Protocols
- HTTP/2

`actix-web` automatically upgrades connections to _HTTP/2_ if possible.

# Negotiation

When either of the `rustls` or `openssl` features are enabled, `HttpServer` provides the [`bind_rustls()`](https://docs.rs/actix-web/4/actix_web/struct.HttpServer.html#method.bind_rustls_0_22) method and [`bind_openssl()`](https://docs.rs/actix-web/4/actix_web/struct.HttpServer.html#method.bind_openssl) methods, respectively.

```toml
[dependencies]

actix-web = { version = "4", features = ["rustls-0_23"] }

rustls = "0.23"

rustls-pemfile = "2"
```

```rust
use actix_web::{web, App, HttpRequest, HttpServer, Responder};

async fn index(_req: HttpRequest) -> impl Responder {

    "Hello TLS World!"

}

#[actix_web::main]

async fn main() -> std::io::Result<()> {

    rustls::crypto::aws_lc_rs::default_provider()

        .install_default()

        .unwrap();

    let mut certs_file = BufReader::new(File::open("cert.pem").unwrap());

    let mut key_file = BufReader::new(File::open("key.pem").unwrap());

    // load TLS certs and key

    // to create a self-signed temporary cert for testing:

    // `openssl req -x509 -newkey rsa:4096 -nodes -keyout key.pem -out cert.pem -days 365 -subj '/CN=localhost'`

    let tls_certs = rustls_pemfile::certs(&mut certs_file)

        .collect::<Result<Vec<_>, _>>()

        .unwrap();

    let tls_key = rustls_pemfile::pkcs8_private_keys(&mut key_file)

        .next()

        .unwrap()

        .unwrap();

    // set up TLS config options

    let tls_config = rustls::ServerConfig::builder()

        .with_no_client_auth()

        .with_single_cert(tls_certs, rustls::pki_types::PrivateKeyDer::Pkcs8(tls_key))

        .unwrap();

    HttpServer::new(|| App::new().route("/", web::get().to(index)))

        .bind_rustls_0_23(("127.0.0.1", 8443), tls_config)?

        .run()

        .await

}
```

Upgrades to HTTP/2 described in [RFC 7540 §3.2](https://httpwg.org/specs/rfc7540.html#rfc.section.3.2) are not supported. Starting HTTP/2 with prior knowledge is supported for both cleartext and TLS connections ( [RFC 7540 §3.4](https://httpwg.org/specs/rfc7540.html#rfc.section.3.4)) (when using the lower level `actix-http` service builders).

> Check out [the TLS examples](https://github.com/actix/examples/tree/master/https-tls) for concrete example.

[Edit this page](https://github.com/actix/actix-website/edit/main/docs/http2.md)

[Previous\\
\\
WebSockets](https://actix.rs/docs/websockets) [Next\\
\\
Auto-Reloading](https://actix.rs/docs/autoreload)

Copyright © 2026 The Actix Team