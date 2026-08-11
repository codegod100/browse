//! Feed our bundled CA certificates into libgit2's OpenSSL trust store,
//! entirely in memory.
//!
//! Android's vendored OpenSSL is built with `no-stdio` — openssl-src's
//! build.rs unconditionally passes that flag for any `android` target (to
//! work around an old NDK build failure, per its own comment), which
//! disables OpenSSL's file-based BIOs. That means libgit2's normal
//! certificate API, `git2::opts::set_ssl_cert_file` (backed by
//! `SSL_CTX_load_verify_locations` → `BIO_new_file`), can never succeed
//! here — regardless of the path or file content, it always fails with a
//! generic wrapped error (OpenSSL library "x509 certificate routines",
//! reason "BIO lib") — and every https repo fetch then fails certificate
//! verification (libgit2 `GIT_ECERTIFICATE`, code -17). Confirmed on
//! device: the file we write is present with the right byte count, and a
//! plain `openssl` CLI parses it fine — the failure is specifically
//! `BIO_new_file` being unavailable in this build, not the cert data.
//!
//! Work around it by parsing the bundle from memory instead (the `openssl`
//! crate's PEM parsing uses a memory BIO, unaffected by `no-stdio`) and
//! adding each certificate straight into libgit2's cert store, one at a
//! time, via its internal `git_openssl__add_x509_cert`. That function is
//! declared in libgit2's `src/libgit2/streams/openssl.h` — not a public
//! header, so the `git2` crate doesn't bind it — but it's an ordinary
//! externally-linked C symbol already present in the static lib linked
//! into this binary, so we can call it directly.

use std::ffi::c_void;
use std::os::raw::c_int;

extern "C" {
    fn git_openssl__add_x509_cert(cert: *mut c_void) -> c_int;
}

/// Parse `pem` (a file of concatenated PEM certificates) and add each one
/// to libgit2's OpenSSL cert store. Returns the number accepted.
///
/// Must run after libgit2's OpenSSL context has been initialized — any
/// prior `git2` call (including a failed one) is enough, since that
/// context is set up eagerly on libgit2's first global init, not lazily
/// on success.
pub fn install_ca_bundle(pem: &[u8]) -> Result<usize, String> {
    let certs = openssl::x509::X509::stack_from_pem(pem).map_err(|e| e.to_string())?;
    let mut added = 0usize;
    for cert in &certs {
        // `X509_STORE_add_cert` (which this calls) increments the store's
        // own reference rather than taking ownership, so `cert` still gets
        // freed normally when it drops at the end of this loop body.
        let ptr = foreign_types::ForeignTypeRef::as_ptr(cert.as_ref()) as *mut c_void;
        // SAFETY: `ptr` is a valid, live `X509*` for the duration of this
        // call (owned by `cert`, which outlives it). libgit2's OpenSSL
        // context was already initialized by an earlier `git2` call.
        if unsafe { git_openssl__add_x509_cert(ptr) } == 0 {
            added += 1;
        }
    }
    if added == 0 {
        return Err("no certificates were accepted into libgit2's trust store".into());
    }
    Ok(added)
}
