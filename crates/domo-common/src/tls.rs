//! rustls client configuration against the one organization root.
//!
//! # The crypto-provider trap (P-10)
//!
//! Both `aws-lc-rs` and `ring` end up in this workspace's dependency graph
//! (the AXIAM SDK's reqwest stack pulls one, rcgen pulls the other). When more
//! than one provider is compiled in, rustls refuses to guess: every
//! `ClientConfig::builder()` call panics with "no process-level CryptoProvider
//! available" until a default is installed explicitly.
//!
//! [`install_crypto_provider`] is therefore the FIRST statement of every
//! binary's `main` in this workspace. It is idempotent.

use std::sync::Arc;

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore};

/// Install the process-wide rustls crypto provider.
///
/// Call this first in `main`, before any TLS configuration is built. Safe to
/// call more than once: a second call is a no-op rather than an error.
pub fn install_crypto_provider() {
    // `install_default` returns Err if a provider is already installed, which
    // is precisely the idempotent case we want to swallow.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Parse a PEM bundle into a [`RootCertStore`] containing only our root.
///
/// The demo has exactly one trust anchor, so this deliberately does NOT fall
/// back to the platform trust store: a certificate that does not chain to the
/// organization root must fail, not quietly succeed against a public CA.
pub fn root_store(root_pem: &[u8]) -> Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    let mut added = 0usize;
    for cert in rustls_pemfile::certs(&mut &root_pem[..]) {
        let cert = cert.context("malformed PEM in root CA bundle")?;
        store.add(cert).context("rejected root CA certificate")?;
        added += 1;
    }
    anyhow::ensure!(added > 0, "root CA bundle contained no certificates");
    Ok(store)
}

/// A client config that trusts the organization root and presents no identity.
pub fn client_config(root_pem: &[u8]) -> Result<ClientConfig> {
    Ok(ClientConfig::builder()
        .with_root_certificates(root_store(root_pem)?)
        .with_no_client_auth())
}

/// A client config that trusts the organization root and presents an mTLS
/// identity.
///
/// `chain_pem` must be the leaf followed by any intermediates (the tenant
/// signing CA); `key_pem` must be PKCS#8.
pub fn client_config_with_identity(
    root_pem: &[u8],
    chain_pem: &[u8],
    key_pem: &[u8],
) -> Result<ClientConfig> {
    let chain: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut &chain_pem[..])
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("malformed PEM in client certificate chain")?;
    anyhow::ensure!(!chain.is_empty(), "client certificate chain was empty");

    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut &key_pem[..])
        .context("malformed PEM in client private key")?
        .context("no private key found (expected a PKCS#8 'BEGIN PRIVATE KEY' block)")?;

    ClientConfig::builder()
        .with_root_certificates(root_store(root_pem)?)
        .with_client_auth_cert(chain, key)
        .context("rustls rejected the client identity")
}

/// The same as [`client_config_with_identity`], wrapped for transports that
/// want a shared handle (notably `rumqttc`'s `TlsConfiguration::Rustls`).
pub fn shared_client_config_with_identity(
    root_pem: &[u8],
    chain_pem: &[u8],
    key_pem: &[u8],
) -> Result<Arc<ClientConfig>> {
    Ok(Arc::new(client_config_with_identity(
        root_pem, chain_pem, key_pem,
    )?))
}
