use std::sync::Arc;

use rustls::{
    DigitallySignedStruct, RootCertStore, SignatureScheme,
    client::{WebPkiServerVerifier, danger},
    pki_types,
};

#[derive(Debug)]
pub struct AllowUnknownIssuerVerification {
    inner: Arc<WebPkiServerVerifier>,
}

impl AllowUnknownIssuerVerification {
    pub fn new() -> Arc<Self> {
        let roots = Arc::new(RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        });
        let inner = WebPkiServerVerifier::builder(roots).build().unwrap();
        Arc::new(Self { inner })
    }
}

impl danger::ServerCertVerifier for AllowUnknownIssuerVerification {
    fn verify_server_cert(
        &self,
        end_entity: &pki_types::CertificateDer<'_>,
        intermediates: &[pki_types::CertificateDer<'_>],
        server_name: &pki_types::ServerName<'_>,
        ocsp: &[u8],
        now: pki_types::UnixTime,
    ) -> Result<danger::ServerCertVerified, rustls::Error> {
        match self
            .inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp, now)
        {
            Ok(scv) => Ok(scv),
            Err(rustls::Error::InvalidCertificate(cert_error)) => {
                if let rustls::CertificateError::UnknownIssuer = cert_error {
                    Ok(danger::ServerCertVerified::assertion())
                } else {
                    Err(rustls::Error::InvalidCertificate(cert_error))
                }
            }
            Err(e) => Err(e),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &pki_types::CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<danger::HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}
