# Rust examples

Run these examples from the repository root:

```bash
cargo run -p wyrd-rust-examples --bin client_request
```

- `client_request.rs` makes one authenticated request through a `WyrdClient`
  built from the ambient configuration.
- `transport_http.rs` and `transport_grpc.rs` point a client's HTTP and gRPC
  planes at explicit endpoints without any network call.
- `support_desk` runs the [support desk](../support-desk/README.md) against
  `WYRD_SERVER_URL`.
