use crate::connection::Connection;
use crate::util::refined_tcp_stream::Stream as RefinedStream;
use std::error::Error;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr};
use std::sync::{Arc, Mutex};
use zeroize::Zeroizing;

/// A wrapper around an owned Rustls connection and corresponding stream.
///
/// Uses an internal Mutex to permit disparate reader & writer threads to access the stream independently.
pub(crate) struct RustlsStream(
    Arc<Mutex<rustls::StreamOwned<rustls::ServerConnection, Connection>>>,
);

impl RustlsStream {
    pub(crate) fn peer_addr(&mut self) -> std::io::Result<Option<SocketAddr>> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .sock
            .peer_addr()
    }

    pub(crate) fn shutdown(&mut self, how: Shutdown) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .sock
            .shutdown(how)
    }
}

impl Clone for RustlsStream {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Read for RustlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .read(buf)
    }
}

impl Write for RustlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("Failed to lock SSL stream mutex")
            .flush()
    }
}

pub(crate) struct RustlsContext(Arc<rustls::ServerConfig>);

impl RustlsContext {
    // rmsvc 补丁（2026-10-10）：上游这段写给 rustls 0.20 + rustls-pemfile 0.2；0.20 受 RUSTSEC-2024-0336
    // （握手中收到 close_notify 时 `complete_io` 死循环）影响且无补丁版，改写成 rustls 0.23 的 API：
    // - 证书/私钥用 rustls 再导出的 pki-types 解 PEM（PKCS#8 / PKCS#1 / SEC1 都认，取第一把）；
    // - 显式用 ring 后端（与 ureq 同一份，不依赖"进程默认 provider"）、安全缺省协议版本（TLS 1.2 + 1.3）；
    // - 私钥不合法时返回错误（上游这里 `expect` 直接 panic）。
    pub(crate) fn from_pem(
        certificates: Vec<u8>,
        private_key: Zeroizing<Vec<u8>>,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};

        let certificate_chain = CertificateDer::pem_slice_iter(&certificates)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Couldn't parse certificate chain: {e}"))?;
        if certificate_chain.is_empty() {
            return Err("Couldn't extract certificate chain from config.".into());
        }

        let private_key = PrivateKeyDer::from_pem_slice(private_key.as_slice())
            .map_err(|e| format!("Couldn't parse private key (encrypted keys are not supported): {e}"))?;

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls_conf = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(certificate_chain, private_key)?;

        Ok(Self(Arc::new(tls_conf)))
    }

    pub(crate) fn accept(
        &self,
        stream: Connection,
    ) -> Result<RustlsStream, Box<dyn Error + Send + Sync + 'static>> {
        let connection = rustls::ServerConnection::new(self.0.clone())?;
        Ok(RustlsStream(Arc::new(Mutex::new(
            rustls::StreamOwned::new(connection, stream),
        ))))
    }
}

impl From<RustlsStream> for RefinedStream {
    fn from(stream: RustlsStream) -> Self {
        Self::Https(stream)
    }
}
