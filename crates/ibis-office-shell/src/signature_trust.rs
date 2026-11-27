// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Whether the certificate a digital signature was made with can be trusted (GitHub issue #232, stage 2;
//! `tasks/doc/design/232-stage2-trust.md`). The chain, revocation and validity period are judged by the operating system's own
//! certificate verification and trust store (user decision A): Windows `CertGetCertificateChain`. Other environments say
//! [`Trust::NotChecked`]. Nothing here verifies a chain by itself.

/// What the operating system says of the signer's certificate, the most serious thing when there are several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Trust {
    /// The chain ends at a root the system trusts; not revoked, within its validity period.
    Trusted,
    /// Revocation could not be looked up (offline, the distribution point does not answer); nothing else is wrong.
    RevocationUnknown,
    /// The certificate is outside its validity period (not yet valid, or expired).
    Expired,
    /// The chain stops before a root (an issuer's certificate is missing).
    PartialChain,
    /// The root is not trusted (a self-signed certificate that is not in the trust store, or one that is distrusted).
    UntrustedRoot,
    /// Revoked.
    Revoked,
    /// This environment cannot verify a chain (the web version, an OS without an implementation, no certificate, unreadable).
    NotChecked,
    /// The call to the system failed, or it reported an error that is none of the above.
    Failed,
}

// `CERT_TRUST_*` error bits (wincrypt.h). Kept here, not taken from the `windows` crate, so that the mapping is tested everywhere.
const IS_NOT_TIME_VALID: u32 = 0x1;
const IS_REVOKED: u32 = 0x4;
const IS_NOT_SIGNATURE_VALID: u32 = 0x8;
const IS_UNTRUSTED_ROOT: u32 = 0x20;
const REVOCATION_STATUS_UNKNOWN: u32 = 0x40;
const IS_PARTIAL_CHAIN: u32 = 0x10000;
const IS_EXPLICIT_DISTRUST: u32 = 0x0400_0000;
const IS_OFFLINE_REVOCATION: u32 = 0x0100_0000;

/// A chain's `TrustStatus.dwErrorStatus` as a [`Trust`]: the most serious of the bits that are set.
pub fn classify(error_status: u32) -> Trust {
    let set = |bits: u32| error_status & bits != 0;
    if set(IS_REVOKED) {
        Trust::Revoked
    } else if set(IS_UNTRUSTED_ROOT | IS_EXPLICIT_DISTRUST) {
        Trust::UntrustedRoot
    } else if set(IS_PARTIAL_CHAIN) {
        Trust::PartialChain
    } else if set(IS_NOT_TIME_VALID) {
        Trust::Expired
    } else if set(IS_NOT_SIGNATURE_VALID) {
        Trust::Failed
    } else if set(REVOCATION_STATUS_UNKNOWN | IS_OFFLINE_REVOCATION) {
        Trust::RevocationUnknown
    } else {
        // Another error (a usage or constraint problem) is not something to call trusted.
        if error_status == 0 { Trust::Trusted } else { Trust::Failed }
    }
}

/// The signer's certificate (the first of `certificates`, DER) against the system's trust; the rest are the chain's intermediate
/// certificates as the signature carried them. May reach the network (the system fetches revocation lists), at most a few seconds.
pub fn evaluate(certificates: &[Vec<u8>]) -> Trust {
    if certificates.is_empty() {
        return Trust::NotChecked;
    }
    platform::evaluate(certificates)
}

#[cfg(windows)]
mod platform {
    use super::{Trust, classify};
    use windows::Win32::Security::Cryptography::*;

    /// Upper bound for the system's fetch of a revocation list or an issuer certificate, in milliseconds.
    const URL_RETRIEVAL_TIMEOUT_MS: u32 = 5000;

    pub fn evaluate(certificates: &[Vec<u8>]) -> Trust {
        unsafe {
            let leaf = CertCreateCertificateContext(X509_ASN_ENCODING, &certificates[0]);
            if leaf.is_null() {
                return Trust::NotChecked; // not a certificate
            }
            let store = CertOpenStore(CERT_STORE_PROV_MEMORY, CERT_QUERY_ENCODING_TYPE(0), None, CERT_OPEN_STORE_FLAGS(0), None);
            let result = match &store {
                Ok(store) => {
                    for der in &certificates[1..] {
                        // An intermediate that cannot be read is skipped: the chain is judged without it.
                        let _ = CertAddEncodedCertificateToStore(Some(*store), X509_ASN_ENCODING, der, CERT_STORE_ADD_USE_EXISTING, None);
                    }
                    chain_status(leaf, Some(*store))
                }
                Err(_) => Trust::Failed,
            };
            if let Ok(store) = store {
                let _ = CertCloseStore(Some(store), 0);
            }
            let _ = CertFreeCertificateContext(Some(leaf));
            result
        }
    }

    unsafe fn chain_status(leaf: *const CERT_CONTEXT, extra: Option<HCERTSTORE>) -> Trust {
        let para = CERT_CHAIN_PARA { cbSize: std::mem::size_of::<CERT_CHAIN_PARA>() as u32, dwUrlRetrievalTimeout: URL_RETRIEVAL_TIMEOUT_MS, ..Default::default() };
        let mut chain: *mut CERT_CHAIN_CONTEXT = std::ptr::null_mut();
        // Time: now (none). The default engine of the current user, so the user's and the machine's trust stores and policies apply.
        let ok = unsafe { CertGetCertificateChain(None, leaf, None, extra, &para, CERT_CHAIN_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, None, &mut chain) };
        if ok.is_err() || chain.is_null() {
            return Trust::Failed;
        }
        let status = unsafe { (*chain).TrustStatus.dwErrorStatus };
        unsafe { CertFreeCertificateChain(chain) };
        classify(status)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Trust;

    // macOS: SecTrust (`SecTrustCreateWithCertificates` + `SecTrustEvaluateWithError`), to be written on a Mac (design, section 3).
    // Linux and the web version: no system verification is used (design, section 3).
    pub fn evaluate(_certificates: &[Vec<u8>]) -> Trust {
        Trust::NotChecked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_picks_the_most_serious_bit() {
        assert_eq!(classify(0), Trust::Trusted);
        assert_eq!(classify(IS_REVOKED), Trust::Revoked);
        assert_eq!(classify(IS_UNTRUSTED_ROOT), Trust::UntrustedRoot);
        assert_eq!(classify(IS_EXPLICIT_DISTRUST), Trust::UntrustedRoot);
        assert_eq!(classify(IS_PARTIAL_CHAIN), Trust::PartialChain);
        assert_eq!(classify(IS_NOT_TIME_VALID), Trust::Expired);
        assert_eq!(classify(REVOCATION_STATUS_UNKNOWN), Trust::RevocationUnknown);
        assert_eq!(classify(IS_OFFLINE_REVOCATION | REVOCATION_STATUS_UNKNOWN), Trust::RevocationUnknown);
        assert_eq!(classify(IS_NOT_SIGNATURE_VALID), Trust::Failed);
        assert_eq!(classify(0x10), Trust::Failed, "IS_NOT_VALID_FOR_USAGE is not called trusted");
        assert_eq!(classify(IS_REVOKED | IS_UNTRUSTED_ROOT | IS_NOT_TIME_VALID), Trust::Revoked);
        assert_eq!(classify(IS_UNTRUSTED_ROOT | IS_NOT_TIME_VALID | REVOCATION_STATUS_UNKNOWN), Trust::UntrustedRoot);
        assert_eq!(classify(IS_NOT_TIME_VALID | REVOCATION_STATUS_UNKNOWN), Trust::Expired);
    }

    #[test]
    fn no_certificate_is_not_checked() {
        assert_eq!(evaluate(&[]), Trust::NotChecked);
        assert_eq!(evaluate(&[b"not a certificate".to_vec()]), Trust::NotChecked);
    }

    /// The fixtures are signed with a self-signed certificate that is in nobody's trust store.
    #[cfg(windows)]
    #[test]
    fn a_self_signed_fixture_is_not_trusted() {
        use ibis_formats::ibis_ooxml::Package;
        use ibis_formats::ibis_ooxml::signature::{Inspection, inspect};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in ["signed-docx-paste-source.docx", "signed-xlsx-sales.xlsx", "signed-text-outline-glow.pptx"] {
            let bytes = std::fs::read(root.join("test/signature").join(name)).unwrap();
            let Inspection::Signed(reports) = inspect(&Package::open(bytes).unwrap()) else { panic!("{name}: signed") };
            let trust = evaluate(&reports[0].certificates);
            assert!(matches!(trust, Trust::UntrustedRoot | Trust::Expired), "{name}: {trust:?}");
        }
    }
}
