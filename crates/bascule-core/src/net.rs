//! One HTTP agent for every request: Mozilla's roots plus the system's (machines behind a company
//! proxy add their own authority there), and the proxy given by the environment (`HTTPS_PROXY`...).

use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let mut builder = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .try_proxy_from_env(true);
        if let Some(tls) = tls_config() {
            builder = builder.tls_config(Arc::new(tls));
        }
        builder.build()
    })
}

fn tls_config() -> Option<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let native = rustls_native_certs::load_native_certs();
    let _ = roots.add_parsable_certificates(native.certs);
    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .ok()
        .map(|b| b.with_root_certificates(roots).with_no_client_auth())
}
