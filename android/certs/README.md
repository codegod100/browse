# Bundled CA certificates

`cacert.pem` is the standard Mozilla CA root bundle (as shipped by nixpkgs'
`cacert` package, i.e. the same trust store `/etc/ssl/certs/ca-certificates.crt`
symlinks to on a NixOS host). It's embedded into the Android build and
extracted to app-private storage at startup so vendored OpenSSL — which has
no system trust store to fall back on inside the app sandbox — can verify
TLS certificates for the web-seed gateway fetch (see `seed_gateways` in
`../src/rad.rs`). See `SSL_CERT_FILE` handling in `../src/lib.rs`.

To refresh: on a Nix system, `cp /etc/ssl/certs/ca-certificates.crt cacert.pem`.
Elsewhere, use curl's extract of the same Mozilla bundle: https://curl.se/ca/cacert.pem.
