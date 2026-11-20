// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Looking at the digital signatures of an OPC package (`.docx`, `.xlsx`, `.pptx`): ECMA-376 Part 2, "Digital Signatures"
//! (XML-DSig, W3C xmldsig-core, with the package's own reference rules).
//!
//! What it tells: whether the package has signatures (the `digital-signature/origin` relationship, the signature parts under it),
//! and for each signature whether its structure is well formed, whether the digest of each thing it covers (the parts it lists, in
//! its Manifest) is what the signature says, and whether the signature value is a valid signature of `SignedInfo` by the key of the
//! certificate the signature carries.
//!
//! What it does not tell: whether the certificate can be trusted (no chain, no revocation, no validity period, no policy), whether
//! there is a time stamp, whether the signature covers the whole document (a part the signature does not list is not reported),
//! and so whether the signature is "valid" in the sense of a signing law. A result of [`Status::Consistent`] means only that the
//! numbers in the signature agree with the file. Nothing is fetched: a reference to anything outside the package is reported as
//! not inspectable.
//!
//! Done from the specifications; not yet compared with signatures made by Office itself (see the tests for what was made by hand).

mod c14n;
mod x509;

use crate::package::{Package, resolve};
use crate::xml::{Document, NodeId};

pub const ORIGIN_RELATIONSHIP: &str = "http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/origin";
pub const SIGNATURE_RELATIONSHIP: &str = "http://schemas.openxmlformats.org/package/2006/relationships/digital-signature/signature";
const NS_DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";
const RELATIONSHIP_TRANSFORM: &str = "http://schemas.openxmlformats.org/package/2006/RelationshipTransform";
const RELATIONSHIPS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const SIGNATURE_DIRECTORY: &str = "_xmlsignatures/";

/// What a package's signatures are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inspection {
    /// No signature.
    Unsigned,
    /// The package says it has signatures but the way they are kept is broken.
    Malformed(Malformed),
    /// One report per signature part.
    Signed(Vec<SignatureReport>),
}

/// How a signature's (or the package's) structure is broken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Malformed {
    /// The relationships of the package or of the origin part cannot be read.
    Relationships,
    /// Signature parts are there but nothing in the package points to them.
    OriginMissing,
    /// The origin relationship points at a part that is not in the file.
    OriginPartMissing,
    /// A signature relationship points at a part that is not in the file.
    SignaturePartMissing,
    /// The signature part cannot be read.
    Unreadable,
    /// The signature part is not well-formed XML.
    Xml,
    /// The root is not an XML-DSig `Signature`.
    NotSignature,
    NoSignedInfo,
    NoSignatureValue,
    /// `SignedInfo` has no `Reference`.
    NoReference,
    /// A `Reference` without its `DigestMethod` or `DigestValue`.
    BadReference,
    /// A `DigestValue` or the `SignatureValue` is not base64.
    BadBase64,
}

/// The result of one comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// The value computed from the file is what the signature says.
    Match,
    /// It is not.
    Mismatch,
    /// A part the signature covers is not in the file.
    PartMissing,
    /// An algorithm, transform or key type this does not handle (its identifier).
    Unsupported(String),
    /// It could not be done.
    Uninspectable(Reason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The reference points outside the package (a URL); it is not fetched.
    External,
    /// The signature carries no certificate or key.
    NoKey,
    /// The certificate cannot be read.
    BadKey,
    /// A same-document reference (`#id`) names nothing.
    TargetMissing,
    /// A transform needs XML and the part is not.
    NotXml,
}

/// The overall result of one signature, the most serious of its checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    /// The signature value and every digest agree with the file (and nothing more is known: see the module).
    Consistent,
    /// Something could not be checked.
    Uninspectable,
    /// Something is not handled.
    Unsupported,
    /// A digest or the signature value does not agree with the file, or a covered part is gone.
    Mismatch,
    /// The signature's structure is broken.
    Malformed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceReport {
    /// The URI as written (for a part, `/word/document.xml?ContentType=…`).
    pub uri: String,
    /// Listed in the signature's Manifest (what the package's parts are) rather than directly in `SignedInfo`.
    pub in_manifest: bool,
    pub check: Check,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureReport {
    /// The signature part's name.
    pub part: String,
    pub status: Status,
    /// Set when `status` is `Malformed`.
    pub malformed: Option<Malformed>,
    /// The subject name of the certificate, as it is written there (not checked to be anybody).
    pub signer: Option<String>,
    /// The signature value against `SignedInfo` (None when malformed).
    pub signature_value: Option<Check>,
    pub references: Vec<ReferenceReport>,
}

/// Whether the package has at least one signature part (cheap: the two relationship parts are read).
pub fn has_signature(package: &Package) -> bool {
    // A broken way of keeping signatures counts: the file is not one that can be edited and saved without notice.
    signature_parts(package).map_or(true, |parts| !parts.is_empty())
}

fn signature_parts(package: &Package) -> Result<Vec<String>, Malformed> {
    let rels = package.relationships("").map_err(|_| Malformed::Relationships)?;
    let Some(origin) = rels.iter().find(|r| r.kind == ORIGIN_RELATIONSHIP && !r.external) else {
        // Parts under `_xmlsignatures/` that no relationship leads to.
        return if package.names().any(|n| n.starts_with(SIGNATURE_DIRECTORY) && !n.ends_with('/')) { Err(Malformed::OriginMissing) } else { Ok(Vec::new()) };
    };
    let origin_part = resolve("", &origin.target);
    if !package.contains(&origin_part) {
        return Err(Malformed::OriginPartMissing);
    }
    let dir = origin_part.rfind('/').map_or("", |i| &origin_part[..i]);
    let rels = package.relationships(&origin_part).map_err(|_| Malformed::Relationships)?;
    let mut parts = Vec::new();
    for r in rels.iter().filter(|r| r.kind == SIGNATURE_RELATIONSHIP && !r.external) {
        let part = resolve(dir, &r.target);
        if !package.contains(&part) {
            return Err(Malformed::SignaturePartMissing);
        }
        parts.push(part);
    }
    Ok(parts)
}

/// Take every digital signature out of `package` (what Office does when a signed file is edited and saved): the parts under
/// `_xmlsignatures/`, the origin part and the signature parts the relationships lead to (with their relationship parts), the
/// package's `digital-signature/origin` relationship, and their entries in `[Content_Types].xml` (an `Override` of each part, and
/// the `Default` of an extension no remaining part has). Everything else is untouched. Returns whether there was anything to take out.
pub fn strip(package: &mut Package) -> Result<bool, crate::Error> {
    let mut doomed: Vec<String> = package.names().filter(|n| n.starts_with(SIGNATURE_DIRECTORY)).map(str::to_string).collect();
    let root_rels = package.relationships("")?;
    let origins: Vec<_> = root_rels.iter().filter(|r| r.kind == ORIGIN_RELATIONSHIP).collect();
    for origin in &origins {
        if origin.external {
            continue;
        }
        let origin_part = resolve("", &origin.target);
        if package.contains(&origin_part) {
            let dir = origin_part.rfind('/').map_or("", |i| &origin_part[..i]);
            for r in package.relationships(&origin_part)?.iter().filter(|r| r.kind == SIGNATURE_RELATIONSHIP && !r.external) {
                doomed.push(resolve(dir, &r.target));
            }
            doomed.push(crate::package::relationships_part(&origin_part));
            doomed.push(origin_part);
        }
    }
    doomed.sort();
    doomed.dedup();
    doomed.retain(|n| package.contains(n));
    if doomed.is_empty() && origins.is_empty() {
        return Ok(false);
    }
    for origin in &origins {
        package.remove_relationship("", &origin.id)?;
    }
    let mut extensions: Vec<String> = Vec::new();
    for name in &doomed {
        package.remove_override(name)?;
        if let Some((_, e)) = name.rsplit_once('.') {
            extensions.push(e.to_ascii_lowercase());
        }
        package.remove(name);
    }
    extensions.sort();
    extensions.dedup();
    for e in extensions {
        // A `Default` only for an extension no part is left with (an `.xml` or `.rels` default stays: the other parts use it).
        if !package.names().any(|n| n.rsplit_once('.').is_some_and(|(_, x)| x.eq_ignore_ascii_case(&e))) {
            package.remove_default(&e)?;
        }
    }
    Ok(true)
}

/// [`strip`] on a package's bytes: `Ok(None)` when the file is not a package or has no signature (nothing to change).
pub fn strip_bytes(bytes: &[u8]) -> Result<Option<Vec<u8>>, crate::Error> {
    let Ok(mut package) = Package::open(bytes.to_vec()) else { return Ok(None) };
    if !strip(&mut package)? {
        return Ok(None);
    }
    Ok(Some(package.save(Default::default())?))
}

/// Inspect every signature of `package`.
pub fn inspect(package: &Package) -> Inspection {
    match signature_parts(package) {
        Err(m) => Inspection::Malformed(m),
        Ok(parts) if parts.is_empty() => Inspection::Unsigned,
        Ok(parts) => Inspection::Signed(parts.into_iter().map(|p| inspect_signature(package, &p)).collect()),
    }
}

fn inspect_signature(package: &Package, part: &str) -> SignatureReport {
    match analyse(package, part) {
        Err(m) => SignatureReport { part: part.to_string(), status: Status::Malformed, malformed: Some(m), signer: None, signature_value: None, references: Vec::new() },
        Ok((signer, value, references)) => {
            let mut status = Status::Consistent;
            for check in std::iter::once(&value).chain(references.iter().map(|r| &r.check)) {
                status = status.max(match check {
                    Check::Match => Status::Consistent,
                    Check::Uninspectable(_) => Status::Uninspectable,
                    Check::Unsupported(_) => Status::Unsupported,
                    Check::Mismatch | Check::PartMissing => Status::Mismatch,
                });
            }
            SignatureReport { part: part.to_string(), status, malformed: None, signer, signature_value: Some(value), references }
        }
    }
}

fn analyse(package: &Package, part: &str) -> Result<(Option<String>, Check, Vec<ReferenceReport>), Malformed> {
    let bytes = package.read(part).map_err(|_| Malformed::Unreadable)?;
    let doc = Document::parse(&bytes).map_err(|_| Malformed::Xml)?;
    let root = doc.root_element().ok_or(Malformed::NotSignature)?;
    if doc.local_name(root) != "Signature" || doc.namespace(root) != Some(NS_DSIG) {
        return Err(Malformed::NotSignature);
    }
    let signed_info = doc.child(root, "SignedInfo").ok_or(Malformed::NoSignedInfo)?;
    let value_element = doc.child(root, "SignatureValue").ok_or(Malformed::NoSignatureValue)?;
    let signature_value = crate::base64::decode(&doc.text(value_element)).ok_or(Malformed::BadBase64)?;
    let reference_elements: Vec<NodeId> = doc.children_named(signed_info, "Reference").collect();
    if reference_elements.is_empty() {
        return Err(Malformed::NoReference);
    }
    let mut references = Vec::new();
    let mut manifests: Vec<NodeId> = Vec::new();
    for &r in &reference_elements {
        references.push(check_reference(package, &doc, r, false)?);
        if let Some(target) = doc.attr(r, "URI").and_then(|u| u.strip_prefix('#')).and_then(|id| find_by_id(&doc, root, id)) {
            for m in std::iter::once(target).chain(doc.descendants(target)).filter(|&n| doc.is_element(n) && doc.local_name(n) == "Manifest") {
                if !manifests.contains(&m) {
                    manifests.push(m);
                }
            }
        }
    }
    for m in manifests {
        for r in doc.children_named(m, "Reference").collect::<Vec<_>>() {
            references.push(check_reference(package, &doc, r, true)?);
        }
    }
    let (signer, value) = check_signature_value(&doc, root, signed_info, &signature_value);
    Ok((signer, value, references))
}

fn find_by_id(doc: &Document, root: NodeId, id: &str) -> Option<NodeId> {
    std::iter::once(root).chain(doc.descendants(root)).find(|&n| doc.is_element(n) && ["Id", "ID", "id"].iter().any(|a| doc.attr(n, a) == Some(id)))
}

enum Algorithm {
    Inclusive,
    Exclusive,
}

fn canonicalization(algorithm: &str) -> Result<Algorithm, Check> {
    match algorithm {
        "http://www.w3.org/TR/2001/REC-xml-c14n-20010315" | "http://www.w3.org/2006/12/xml-c14n11" => Ok(Algorithm::Inclusive),
        "http://www.w3.org/2001/10/xml-exc-c14n#" => Ok(Algorithm::Exclusive),
        other => Err(Check::Unsupported(other.to_string())),
    }
}

fn digest_hash(algorithm: &str) -> Option<x509::Hash> {
    Some(match algorithm {
        "http://www.w3.org/2000/09/xmldsig#sha1" => x509::Hash::Sha1,
        "http://www.w3.org/2001/04/xmlenc#sha256" => x509::Hash::Sha256,
        "http://www.w3.org/2001/04/xmldsig-more#sha384" => x509::Hash::Sha384,
        "http://www.w3.org/2001/04/xmlenc#sha512" => x509::Hash::Sha512,
        _ => return None,
    })
}

enum Cur {
    Bytes(Vec<u8>),
    Xml(Box<Document>),
    Node(NodeId),
}

fn check_reference(package: &Package, doc: &Document, reference: NodeId, in_manifest: bool) -> Result<ReferenceReport, Malformed> {
    let uri = doc.attr(reference, "URI").unwrap_or("").to_string();
    let method = doc.child(reference, "DigestMethod").and_then(|m| doc.attr(m, "Algorithm")).ok_or(Malformed::BadReference)?.to_string();
    let value = doc.child(reference, "DigestValue").ok_or(Malformed::BadReference)?;
    let expected = crate::base64::decode(&doc.text(value)).ok_or(Malformed::BadBase64)?;
    let transforms: Vec<NodeId> = doc.child(reference, "Transforms").map(|t| doc.children_named(t, "Transform").collect()).unwrap_or_default();
    let check = (|| -> Check {
        let Some(hash) = digest_hash(&method) else { return Check::Unsupported(method.clone()) };
        let start = if let Some(id) = uri.strip_prefix('#') {
            let Some(root) = doc.root_element() else { return Check::Uninspectable(Reason::TargetMissing) };
            match find_by_id(doc, root, id) {
                Some(n) => Cur::Node(n),
                None => return Check::Uninspectable(Reason::TargetMissing),
            }
        } else if uri.is_empty() {
            return Check::Unsupported("URI=\"\"".to_string());
        } else if has_scheme(&uri) {
            return Check::Uninspectable(Reason::External);
        } else {
            let name = part_name(&uri);
            if !package.contains(&name) {
                return Check::PartMissing;
            }
            match package.read(&name) {
                Ok(b) => Cur::Bytes(b),
                Err(_) => return Check::PartMissing,
            }
        };
        match transformed(doc, &transforms, start) {
            Ok(bytes) => {
                if hash.digest(&bytes) == expected {
                    Check::Match
                } else {
                    Check::Mismatch
                }
            }
            Err(check) => check,
        }
    })();
    Ok(ReferenceReport { uri, in_manifest, check })
}

fn has_scheme(uri: &str) -> bool {
    match uri.find(':') {
        Some(i) => i > 0 && !uri[..i].contains(['/', '?', '#']) && uri[..i].chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)),
        None => false,
    }
}

/// The part a reference URI names (`/word/document.xml?ContentType=…` is `word/document.xml`).
fn part_name(uri: &str) -> String {
    let path = uri.split(['?', '#']).next().unwrap_or(uri);
    let decoded = percent_decode(path);
    decoded.trim_start_matches('/').to_string()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = ((bytes[i + 1] as char).to_digit(16), (bytes[i + 2] as char).to_digit(16))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn transformed(doc: &Document, transforms: &[NodeId], start: Cur) -> Result<Vec<u8>, Check> {
    let mut cur = start;
    for &t in transforms {
        let algorithm = doc.attr(t, "Algorithm").unwrap_or("");
        match algorithm {
            RELATIONSHIP_TRANSFORM => {
                let Cur::Bytes(bytes) = &cur else { return Err(Check::Unsupported(algorithm.to_string())) };
                cur = Cur::Xml(Box::new(relationship_transform(doc, t, bytes)?));
            }
            other => {
                let exclusive = match canonicalization(other) {
                    Ok(Algorithm::Exclusive) => true,
                    Ok(Algorithm::Inclusive) => false,
                    Err(c) => return Err(c),
                };
                if doc.children_named(t, "InclusiveNamespaces").next().is_some() {
                    return Err(Check::Unsupported("InclusiveNamespaces".to_string()));
                }
                cur = Cur::Bytes(match cur {
                    Cur::Node(n) => c14n::canonicalize_subtree(doc, n, exclusive),
                    Cur::Xml(d) => c14n::canonicalize_document(&d, exclusive),
                    Cur::Bytes(b) => {
                        let d = Document::parse(&b).map_err(|_| Check::Uninspectable(Reason::NotXml))?;
                        c14n::canonicalize_document(&d, exclusive)
                    }
                });
            }
        }
    }
    Ok(match cur {
        Cur::Bytes(b) => b,
        Cur::Xml(d) => c14n::canonicalize_document(&d, false),
        Cur::Node(n) => c14n::canonicalize_subtree(doc, n, false),
    })
}

/// ECMA-376 Part 2, 13.2.4.24: the relationships the transform names (by `SourceId` or `SourceType`), in the order of their `Id`s,
/// as a Relationships part with each relationship's `TargetMode` written out.
fn relationship_transform(doc: &Document, transform: NodeId, rels: &[u8]) -> Result<Document, Check> {
    let rels_doc = Document::parse(rels).map_err(|_| Check::Uninspectable(Reason::NotXml))?;
    let Some(rels_root) = rels_doc.root_element() else { return Err(Check::Uninspectable(Reason::NotXml)) };
    let mut ids: Vec<&str> = Vec::new();
    let mut types: Vec<&str> = Vec::new();
    for n in doc.elements(transform) {
        match doc.local_name(n) {
            "RelationshipReference" => ids.extend(doc.attr(n, "SourceId")),
            "RelationshipsGroupReference" => types.extend(doc.attr(n, "SourceType")),
            _ => {}
        }
    }
    let mut chosen: Vec<(String, String, String, String)> = Vec::new();
    for r in rels_doc.children_named(rels_root, "Relationship") {
        let id = rels_doc.attr(r, "Id").unwrap_or("");
        let kind = rels_doc.attr(r, "Type").unwrap_or("");
        if ids.contains(&id) || types.contains(&kind) {
            let mode = rels_doc.attr(r, "TargetMode").unwrap_or("Internal");
            chosen.push((id.to_string(), kind.to_string(), rels_doc.attr(r, "Target").unwrap_or("").to_string(), mode.to_string()));
        }
    }
    chosen.sort_by(|a, b| a.0.cmp(&b.0));
    let mut text = format!("<Relationships xmlns=\"{RELATIONSHIPS_NS}\">");
    for (id, kind, target, mode) in chosen {
        text.push_str(&format!("<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\" TargetMode=\"{}\"/>", escape(&id), escape(&kind), escape(&target), escape(&mode)));
    }
    text.push_str("</Relationships>");
    Document::parse(text.as_bytes()).map_err(|_| Check::Uninspectable(Reason::NotXml))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}

fn check_signature_value(doc: &Document, root: NodeId, signed_info: NodeId, signature: &[u8]) -> (Option<String>, Check) {
    // The certificates and keys the signature carries.
    let mut certificates: Vec<x509::Certificate> = Vec::new();
    let mut unreadable = false;
    if let Some(key_info) = doc.child(root, "KeyInfo") {
        for n in doc.descendants(key_info) {
            if !doc.is_element(n) {
                continue;
            }
            match doc.local_name(n) {
                "X509Certificate" => match crate::base64::decode(&doc.text(n)).and_then(|der| x509::parse_certificate(&der)) {
                    Some(c) => certificates.push(c),
                    None => unreadable = true,
                },
                "RSAKeyValue" => {
                    let part = |name: &str| doc.child(n, name).and_then(|e| crate::base64::decode(&doc.text(e)));
                    if let (Some(modulus), Some(exponent)) = (part("Modulus"), part("Exponent")) {
                        certificates.push(x509::Certificate { subject: None, key: x509::PublicKey::Rsa { modulus, exponent } });
                    } else {
                        unreadable = true;
                    }
                }
                _ => {}
            }
        }
    }
    let first_signer = certificates.first().and_then(|c| c.subject.clone());

    let method = doc.child(signed_info, "CanonicalizationMethod").and_then(|m| doc.attr(m, "Algorithm")).unwrap_or("");
    let exclusive = match canonicalization(method) {
        Ok(Algorithm::Exclusive) => true,
        Ok(Algorithm::Inclusive) => false,
        Err(c) => return (first_signer, c),
    };
    if doc.child(signed_info, "CanonicalizationMethod").is_some_and(|m| doc.children_named(m, "InclusiveNamespaces").next().is_some()) {
        return (first_signer, Check::Unsupported("InclusiveNamespaces".to_string()));
    }
    let signature_method = doc.child(signed_info, "SignatureMethod").and_then(|m| doc.attr(m, "Algorithm")).unwrap_or("");
    let hash = match signature_method {
        "http://www.w3.org/2000/09/xmldsig#rsa-sha1" => x509::Hash::Sha1,
        "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256" => x509::Hash::Sha256,
        "http://www.w3.org/2001/04/xmldsig-more#rsa-sha384" => x509::Hash::Sha384,
        "http://www.w3.org/2001/04/xmldsig-more#rsa-sha512" => x509::Hash::Sha512,
        other => return (first_signer, Check::Unsupported(other.to_string())),
    };
    if certificates.is_empty() {
        return (first_signer, Check::Uninspectable(if unreadable { Reason::BadKey } else { Reason::NoKey }));
    }
    let message = c14n::canonicalize_subtree(doc, signed_info, exclusive);
    let mut other_key = None;
    let mut any_rsa = false;
    for c in &certificates {
        match &c.key {
            x509::PublicKey::Rsa { modulus, exponent } => {
                any_rsa = true;
                if x509::rsa_pkcs1_v15_verify(modulus, exponent, hash, &message, signature) {
                    return (c.subject.clone().or(first_signer), Check::Match);
                }
            }
            x509::PublicKey::Other(oid) => other_key = Some(oid.clone()),
        }
    }
    if any_rsa {
        (first_signer, Check::Mismatch)
    } else {
        (first_signer, Check::Unsupported(format!("public key {}", other_key.unwrap_or_default())))
    }
}
