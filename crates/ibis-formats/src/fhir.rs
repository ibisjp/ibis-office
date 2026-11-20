// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust HL7 FHIR (Bundle JSON/XML, JP-Core 3文書6情報) and HL7 v2.x
//! faithful document formatter for ibisOffice.
//!
//! Non-SaMD boundary: performs strictly faithful structural transcription and
//! layout formatting into Markdown/DOCX without any diagnostic inference,
//! risk scoring, or treatment recommendation.

use std::fmt::Write as _;

use serde_json::Value;

use crate::Error;

/// Convert HL7 FHIR (Bundle JSON/XML) or HL7 v2.x bytes into structured Markdown
/// if the input matches one of those formats; returns `Ok(None)` otherwise.
pub fn input_to_markdown(data: &[u8]) -> Result<Option<String>, Error> {
    if data.is_empty() || data.starts_with(b"PK\x03\x04") {
        return Ok(None);
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return Ok(None);
    };
    let trimmed = text.trim_start_matches('\u{FEFF}').trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed.starts_with('{') && looks_like_fhir_json(trimmed) {
        return render_fhir_json_to_markdown(trimmed).map(Some);
    }
    if trimmed.starts_with('<') && looks_like_cda_xml(trimmed) {
        return render_cda_xml_to_markdown(trimmed).map(Some);
    }
    if trimmed.starts_with('<') && looks_like_fhir_xml(trimmed) {
        return render_fhir_xml_to_markdown(trimmed).map(Some);
    }
    if looks_like_hl7v2(trimmed) {
        return Ok(Some(render_hl7v2_to_markdown(trimmed)));
    }
    Ok(None)
}

/// An HL7 CDA document: the root is `ClinicalDocument` in the namespace `urn:hl7-org:v3`.
fn looks_like_cda_xml(trimmed: &str) -> bool {
    if !trimmed.contains("urn:hl7-org:v3") {
        return false;
    }
    let Ok(doc) = roxmltree::Document::parse(trimmed) else { return false };
    let root = doc.root_element();
    root.tag_name().namespace() == Some("urn:hl7-org:v3") && root.tag_name().name() == "ClinicalDocument"
}

/// Summary of authenticity / signature markers in a FHIR Bundle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FhirAuthenticityReport {
    pub signature_count: usize,
    pub provenance_count: usize,
    pub signers: Vec<String>,
}

/// Inspect `Signature` / `Provenance` elements in a FHIR JSON Bundle.
pub fn inspect_fhir_authenticity(data: &[u8]) -> Result<FhirAuthenticityReport, Error> {
    let text = std::str::from_utf8(data)
        .map_err(|e| Error::new(format!("UTF-8 変換に失敗しました: {e}")))?;
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{FEFF}'))
        .map_err(|e| Error::new(format!("FHIR JSON のパースに失敗しました: {e}")))?;
    Ok(collect_fhir_authenticity(&root))
}

fn collect_fhir_authenticity(root: &Value) -> FhirAuthenticityReport {
    let mut report = FhirAuthenticityReport::default();
    if root.get("signature").is_some() {
        report.signature_count += 1;
    }
    if let Some(entries) = root.get("entry").and_then(Value::as_array) {
        for e in entries {
            let Some(res) = e.get("resource") else { continue };
            if res_type(res) == "Provenance" {
                report.provenance_count += 1;
                if let Some(sigs) = res.get("signature").and_then(Value::as_array) {
                    report.signature_count += sigs.len();
                    for sig in sigs {
                        if let Some(who) = sig
                            .get("who")
                            .and_then(|w| w.get("display").or_else(|| w.get("reference")))
                            .and_then(Value::as_str)
                        {
                            report.signers.push(who.to_string());
                        }
                    }
                }
            }
        }
    }
    report
}

/// The resource types of HL7 FHIR R4 (the names a `resourceType`, or the root element of an XML resource, can have).
const FHIR_R4_RESOURCES: &[&str] = &[
    "Account",
    "ActivityDefinition",
    "AdverseEvent",
    "AllergyIntolerance",
    "Appointment",
    "AppointmentResponse",
    "AuditEvent",
    "Basic",
    "Binary",
    "BiologicallyDerivedProduct",
    "BodyStructure",
    "Bundle",
    "CapabilityStatement",
    "CarePlan",
    "CareTeam",
    "CatalogEntry",
    "ChargeItem",
    "ChargeItemDefinition",
    "Claim",
    "ClaimResponse",
    "ClinicalImpression",
    "CodeSystem",
    "Communication",
    "CommunicationRequest",
    "CompartmentDefinition",
    "Composition",
    "ConceptMap",
    "Condition",
    "Consent",
    "Contract",
    "Coverage",
    "CoverageEligibilityRequest",
    "CoverageEligibilityResponse",
    "DetectedIssue",
    "Device",
    "DeviceDefinition",
    "DeviceMetric",
    "DeviceRequest",
    "DeviceUseStatement",
    "DiagnosticReport",
    "DocumentManifest",
    "DocumentReference",
    "EffectEvidenceSynthesis",
    "Encounter",
    "Endpoint",
    "EnrollmentRequest",
    "EnrollmentResponse",
    "EpisodeOfCare",
    "EventDefinition",
    "Evidence",
    "EvidenceVariable",
    "ExampleScenario",
    "ExplanationOfBenefit",
    "FamilyMemberHistory",
    "Flag",
    "Goal",
    "GraphDefinition",
    "Group",
    "GuidanceResponse",
    "HealthcareService",
    "ImagingStudy",
    "Immunization",
    "ImmunizationEvaluation",
    "ImmunizationRecommendation",
    "ImplementationGuide",
    "InsurancePlan",
    "Invoice",
    "Library",
    "Linkage",
    "List",
    "Location",
    "Measure",
    "MeasureReport",
    "Media",
    "Medication",
    "MedicationAdministration",
    "MedicationDispense",
    "MedicationKnowledge",
    "MedicationRequest",
    "MedicationStatement",
    "MedicinalProduct",
    "MedicinalProductAuthorization",
    "MedicinalProductContraindication",
    "MedicinalProductIndication",
    "MedicinalProductIngredient",
    "MedicinalProductInteraction",
    "MedicinalProductManufactured",
    "MedicinalProductPackaged",
    "MedicinalProductPharmaceutical",
    "MedicinalProductUndesirableEffect",
    "MessageDefinition",
    "MessageHeader",
    "MolecularSequence",
    "NamingSystem",
    "NutritionOrder",
    "Observation",
    "ObservationDefinition",
    "OperationDefinition",
    "OperationOutcome",
    "Organization",
    "OrganizationAffiliation",
    "Parameters",
    "Patient",
    "PaymentNotice",
    "PaymentReconciliation",
    "Person",
    "PlanDefinition",
    "Practitioner",
    "PractitionerRole",
    "Procedure",
    "Provenance",
    "Questionnaire",
    "QuestionnaireResponse",
    "RelatedPerson",
    "RequestGroup",
    "ResearchDefinition",
    "ResearchElementDefinition",
    "ResearchStudy",
    "ResearchSubject",
    "RiskAssessment",
    "RiskEvidenceSynthesis",
    "Schedule",
    "SearchParameter",
    "ServiceRequest",
    "Slot",
    "Specimen",
    "SpecimenDefinition",
    "StructureDefinition",
    "StructureMap",
    "Subscription",
    "Substance",
    "SubstanceNucleicAcid",
    "SubstancePolymer",
    "SubstanceProtein",
    "SubstanceReferenceInformation",
    "SubstanceSourceMaterial",
    "SubstanceSpecification",
    "SupplyDelivery",
    "SupplyRequest",
    "Task",
    "TerminologyCapabilities",
    "TestReport",
    "TestScript",
    "ValueSet",
    "VerificationResult",
    "VisionPrescription",
];

fn is_fhir_resource(name: &str) -> bool {
    FHIR_R4_RESOURCES.binary_search(&name).is_ok()
}

/// A JSON object whose `resourceType` is a resource of FHIR R4 (a JSON file that only mentions one, in a string or deeper, is not).
fn looks_like_fhir_json(trimmed: &str) -> bool {
    if !trimmed.contains("\"resourceType\"") {
        return false;
    }
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(trimmed) else { return false };
    root.get("resourceType").and_then(Value::as_str).is_some_and(is_fhir_resource)
}

/// An XML document whose root is a FHIR R4 resource in the FHIR namespace.
fn looks_like_fhir_xml(trimmed: &str) -> bool {
    if !trimmed.contains("http://hl7.org/fhir") {
        return false;
    }
    let Ok(doc) = roxmltree::Document::parse(trimmed) else { return false };
    let root = doc.root_element();
    root.tag_name().namespace() == Some("http://hl7.org/fhir") && is_fhir_resource(root.tag_name().name())
}

/// An HL7 v2 message: the MSH segment with its separators and at least the fields up to the message type (MSH-9), then segments that begin with their
/// three-character names.
fn looks_like_hl7v2(trimmed: &str) -> bool {
    let mut lines = trimmed.lines().map(str::trim_end).filter(|l| !l.is_empty());
    let Some(first) = lines.next() else { return false };
    if !(first.starts_with("MSH|") && first.len() >= 9 && first[4..].starts_with("^~\\&") && first.split('|').count() >= 9) {
        return false;
    }
    lines.all(|l| {
        let b = l.as_bytes();
        b.len() >= 4 && b[..3].iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) && b[3] == b'|'
    })
}

fn render_fhir_json_to_markdown(text: &str) -> Result<String, Error> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| Error::new(format!("FHIR JSON のパースに失敗しました: {e}")))?;

    let rtype = root
        .get("resourceType")
        .and_then(Value::as_str)
        .unwrap_or("Unknown");
    let resources: Vec<&Value> = if rtype == "Bundle" {
        root.get("entry")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.get("resource"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        vec![&root]
    };

    let composition = resources
        .iter()
        .copied()
        .find(|r| res_type(r) == "Composition");
    let title = composition
        .and_then(|c| c.get("title").and_then(Value::as_str))
        .map_or_else(|| crate::words::text("medical.fhir.default-title"), str::to_string);
    let doc_date = composition
        .and_then(|c| c.get("date").and_then(Value::as_str))
        .or_else(|| root.get("timestamp").and_then(Value::as_str))
        .unwrap_or("");
    let bundle_id = root.get("id").and_then(Value::as_str).unwrap_or("");

    let mut md = String::new();
    let _ = writeln!(md, "---");
    let _ = writeln!(md, "title: {}", yaml_escape(&title));
    let _ = writeln!(md, "fhir_resource_type: {}", yaml_escape(rtype));
    if !bundle_id.is_empty() {
        let _ = writeln!(md, "fhir_id: {}", yaml_escape(bundle_id));
    }
    if !doc_date.is_empty() {
        let _ = writeln!(md, "date: {}", yaml_escape(doc_date));
    }
    let _ = writeln!(md, "---\n");
    let _ = writeln!(md, "# {title}\n");
    let _ = writeln!(md, "> {}\n", crate::words::text("medical.fhir.notice"));
    let auth = collect_fhir_authenticity(&root);
    if auth.signature_count > 0 || auth.provenance_count > 0 {
        let signers = if auth.signers.is_empty() { "-".to_string() } else { auth.signers.join(", ") };
        let _ = writeln!(
            md,
            "> {}\n",
            crate::words::format("medical.fhir.authenticity", &[("provenance", &auth.provenance_count), ("signatures", &auth.signature_count), ("signers", &signers)])
        );
    }

    // Patient(s)
    let patients: Vec<&Value> = resources
        .iter()
        .copied()
        .filter(|r| res_type(r) == "Patient")
        .collect();
    if !patients.is_empty() {
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir.patient-heading"));
        let _ = writeln!(md, "{}", crate::words::text("medical.fhir.patient-head"));
        let _ = writeln!(md, "| --- | --- |");
        for p in patients {
            let name = extract_human_name(p);
            let gender = p.get("gender").and_then(Value::as_str).unwrap_or("-");
            let birth = p.get("birthDate").and_then(Value::as_str).unwrap_or("-");
            let pid = p
                .get("identifier")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|i| i.get("value").and_then(Value::as_str))
                .or_else(|| p.get("id").and_then(Value::as_str))
                .unwrap_or("-");
            let _ = writeln!(md, "| {} | {} |", crate::words::text("medical.fhir.patient-id"), md_cell(pid));
            let _ = writeln!(md, "| {} | {} |", crate::words::text("medical.fhir.patient-name"), md_cell(&name));
            let _ = writeln!(md, "| {} | {} |", crate::words::text("medical.fhir.patient-gender"), md_cell(gender));
            let _ = writeln!(md, "| {} | {} |", crate::words::text("medical.fhir.patient-birth"), md_cell(birth));
        }
        md.push('\n');
    }

    // Organizations / Practitioners
    let orgs: Vec<&Value> = resources
        .iter()
        .copied()
        .filter(|r| matches!(res_type(r), "Organization" | "Practitioner"))
        .collect();
    if !orgs.is_empty() {
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir.issuer-heading"));
        let _ = writeln!(md, "{}", crate::words::text("medical.fhir.issuer-head"));
        let _ = writeln!(md, "| --- | --- |");
        for o in orgs {
            let kind = res_type(o);
            let name = o
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| extract_human_name(o));
            let _ = writeln!(md, "| {kind} | {} |", md_cell(&name));
        }
        md.push('\n');
    }

    // 6情報: 傷病名 (Condition)
    let conditions: Vec<&Value> = resources
        .iter()
        .copied()
        .filter(|r| res_type(r) == "Condition")
        .collect();
    if !conditions.is_empty() {
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir.condition-heading"));
        let _ = writeln!(md, "{}", crate::words::text("medical.fhir.condition-head"));
        let _ = writeln!(md, "| --- | --- | --- | --- |");
        for c in conditions {
            let (disp, code) = extract_codeable(c.get("code"));
            let status = extract_codeable(c.get("clinicalStatus")).0;
            let onset = c
                .get("onsetDateTime")
                .or_else(|| c.get("recordedDate"))
                .and_then(Value::as_str)
                .unwrap_or("-");
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} |",
                md_cell(&disp),
                md_cell(&code),
                md_cell(&status),
                md_cell(onset)
            );
        }
        md.push('\n');
    }

    // 6情報: アレルギー・薬剤禁忌 (AllergyIntolerance)
    let allergies: Vec<&Value> = resources
        .iter()
        .copied()
        .filter(|r| res_type(r) == "AllergyIntolerance")
        .collect();
    if !allergies.is_empty() {
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir.allergy-heading"));
        let _ = writeln!(md, "{}", crate::words::text("medical.fhir.allergy-head"));
        let _ = writeln!(md, "| --- | --- | --- | --- |");
        for a in allergies {
            let (disp, _) = extract_codeable(a.get("code"));
            let cat = a
                .get("category")
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .unwrap_or("-");
            let crit = a.get("criticality").and_then(Value::as_str).unwrap_or("-");
            let status = extract_codeable(a.get("clinicalStatus")).0;
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} |",
                md_cell(&disp),
                md_cell(cat),
                md_cell(crit),
                md_cell(&status)
            );
        }
        md.push('\n');
    }

    // 6情報: 検査・感染症 (Observation)
    let observations: Vec<&Value> = resources
        .iter()
        .copied()
        .filter(|r| res_type(r) == "Observation")
        .collect();
    if !observations.is_empty() {
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir.observation-heading"));
        let _ = writeln!(md, "{}", crate::words::text("medical.fhir.observation-head"));
        let _ = writeln!(md, "| --- | --- | --- | --- |");
        for o in observations {
            let (disp, code) = extract_codeable(o.get("code"));
            let val = extract_observation_value(o);
            let dt = o
                .get("effectiveDateTime")
                .and_then(Value::as_str)
                .unwrap_or("-");
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} |",
                md_cell(&disp),
                md_cell(&code),
                md_cell(&val),
                md_cell(dt)
            );
        }
        md.push('\n');
    }

    // 6情報: 処方 (MedicationRequest / MedicationStatement)
    let meds: Vec<&Value> = resources
        .iter()
        .copied()
        .filter(|r| matches!(res_type(r), "MedicationRequest" | "MedicationStatement"))
        .collect();
    if !meds.is_empty() {
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir.medication-heading"));
        let _ = writeln!(md, "{}", crate::words::text("medical.fhir.medication-head"));
        let _ = writeln!(md, "| --- | --- | --- | --- |");
        for m in meds {
            let (disp, code) = extract_codeable(m.get("medicationCodeableConcept"));
            let dosage = m
                .get("dosageInstruction")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|d| d.get("text").and_then(Value::as_str))
                .unwrap_or("-");
            let status = m.get("status").and_then(Value::as_str).unwrap_or("-");
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} |",
                md_cell(&disp),
                md_cell(&code),
                md_cell(dosage),
                md_cell(status)
            );
        }
        md.push('\n');
    }

    Ok(md)
}

/// Convert HL7 CDA `<ClinicalDocument>` (e.g. 特定健診・特定保健指導 XML) into Markdown.
fn render_cda_xml_to_markdown(text: &str) -> Result<String, Error> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| Error::new(format!("HL7 CDA XML のパースに失敗しました: {e}")))?;
    let root = doc.root_element();
    let title = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "title")
        .and_then(|n| n.text())
        .map_or_else(|| crate::words::text("medical.cda.default-title"), str::to_string);
    let effective_time = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "effectiveTime")
        .and_then(|n| n.attribute("value"))
        .unwrap_or("-");

    let mut md = String::new();
    let _ = writeln!(
        md,
        "---\ntitle: {}\ncda_effective_time: {}\n---\n",
        yaml_escape(&title),
        yaml_escape(effective_time)
    );
    let _ = writeln!(md, "# {title}\n");
    let _ = writeln!(md, "- {}\n", crate::words::format("medical.cda.time", &[("time", &effective_time)]));
    let _ = writeln!(md, "## {}\n", crate::words::text("medical.cda.observation-heading"));
    let _ = writeln!(md, "{}", crate::words::text("medical.cda.observation-head"));
    let _ = writeln!(md, "| --- | --- | --- | --- |");

    for obs in root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "observation")
    {
        let code_node = obs
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "code");
        let disp = code_node
            .and_then(|c| c.attribute("displayName"))
            .unwrap_or("-");
        let code = code_node.and_then(|c| c.attribute("code")).unwrap_or("-");
        let val_node = obs
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "value");
        let val = val_node
            .and_then(|v| v.attribute("value").or_else(|| v.text()))
            .unwrap_or("-");
        let unit = val_node.and_then(|v| v.attribute("unit")).unwrap_or("-");
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} |",
            md_cell(disp),
            md_cell(code),
            md_cell(val),
            md_cell(unit)
        );
    }
    Ok(md)
}

/// Convert a structured Word (`FlowDocument`) presentation created from a
/// medical form (or round-tripped from `input_to_markdown`) back into an HL7
/// FHIR R4 `Bundle` JSON string (`resourceType: "Bundle"`, `type: "document"`).
pub fn flow_to_fhir_bundle_json(
    presentation: &ibis_doc_model::Presentation,
) -> Result<String, Error> {
    use ibis_doc_model::flow::Block;

    let flow = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("DOCX フロードキュメントが存在しません"))?;

    let mut title = crate::words::text("medical.fhir.default-title-docx");
    let mut entries: Vec<Value> = Vec::new();
    let mut current_section = String::new();

    for block in &flow.body {
        match block {
            Block::Paragraph(p) => {
                let text = flow_paragraph_plain_text(p);
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if p.style.as_deref() == Some("Heading1") {
                    title = trimmed.to_string();
                } else if p.style.as_deref().is_some_and(|s| s.starts_with("Heading")) {
                    current_section = trimmed.to_string();
                }
            }
            Block::Table(tbl) => {
                let rows: Vec<Vec<String>> = tbl
                    .rows
                    .iter()
                    .map(|r| {
                        r.cells
                            .iter()
                            .map(|c| {
                                c.blocks
                                    .iter()
                                    .filter_map(|b| match b {
                                        Block::Paragraph(p) => {
                                            Some(flow_paragraph_plain_text(p))
                                        }
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ")
                                    .trim()
                                    .to_string()
                            })
                            .collect()
                    })
                    .collect();
                if rows.len() < 2 {
                    continue;
                }
                if crate::words::is(&current_section, "medical.fhir.patient-heading") {
                    let mut map = serde_json::Map::new();
                    map.insert("resourceType".into(), Value::String("Patient".into()));
                    for row in rows.iter().skip(1) {
                        if row.len() >= 2 {
                            let key = row[0].as_str();
                            if crate::words::same(key, "medical.fhir.patient-id") {
                                map.insert("id".into(), Value::String(row[1].clone()));
                            } else if crate::words::same(key, "medical.fhir.patient-name") {
                                map.insert("name".into(), serde_json::json!([{ "text": row[1] }]));
                            } else if crate::words::same(key, "medical.fhir.patient-gender") {
                                map.insert("gender".into(), Value::String(row[1].clone()));
                            } else if crate::words::same(key, "medical.fhir.patient-birth") {
                                map.insert("birthDate".into(), Value::String(row[1].clone()));
                            }
                        }
                    }
                    entries.push(serde_json::json!({ "resource": Value::Object(map) }));
                } else if current_section.contains("Condition") || current_section.contains("傷病名") { // i18n-ok（日本語で書かれた見出しも読む）
                    for row in rows.iter().skip(1) {
                        if !row.is_empty() {
                            entries.push(serde_json::json!({
                                "resource": {
                                    "resourceType": "Condition",
                                    "code": {
                                        "coding": [{
                                            "code": row.get(1).cloned().unwrap_or_default(),
                                            "display": row[0]
                                        }]
                                    },
                                    "onsetDateTime": row.get(3).cloned().unwrap_or_default()
                                }
                            }));
                        }
                    }
                } else if current_section.contains("Observation") || current_section.contains("検査") { // i18n-ok（日本語で書かれた見出しも読む）
                    for row in rows.iter().skip(1) {
                        if !row.is_empty() {
                            entries.push(serde_json::json!({
                                "resource": {
                                    "resourceType": "Observation",
                                    "code": {
                                        "coding": [{
                                            "code": row.get(1).cloned().unwrap_or_default(),
                                            "display": row[0]
                                        }]
                                    },
                                    "valueString": row.get(2).cloned().unwrap_or_default(),
                                    "effectiveDateTime": row.get(3).cloned().unwrap_or_default()
                                }
                            }));
                        }
                    }
                } else if current_section.contains("Medication") || current_section.contains("処方") { // i18n-ok（日本語で書かれた見出しも読む）
                    for row in rows.iter().skip(1) {
                        if !row.is_empty() {
                            entries.push(serde_json::json!({
                                "resource": {
                                    "resourceType": "MedicationRequest",
                                    "status": row.get(3).cloned().unwrap_or_else(|| "active".to_string()),
                                    "medicationCodeableConcept": {
                                        "coding": [{
                                            "code": row.get(1).cloned().unwrap_or_default(),
                                            "display": row[0]
                                        }]
                                    },
                                    "dosageInstruction": [{ "text": row.get(2).cloned().unwrap_or_default() }]
                                }
                            }));
                        }
                    }
                }
            }
        }
    }

    let mut all_entries = vec![serde_json::json!({
        "resource": {
            "resourceType": "Composition",
            "status": "final",
            "title": title
        }
    })];
    all_entries.extend(entries);

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "document",
        "entry": all_entries
    });
    serde_json::to_string_pretty(&bundle)
        .map_err(|e| Error::new(format!("FHIR Bundle JSON の生成に失敗しました: {e}")))
}

fn render_fhir_xml_to_markdown(text: &str) -> Result<String, Error> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| Error::new(format!("FHIR XML のパースに失敗しました: {e}")))?;
    let root = doc.root_element();
    let root_name = root.tag_name().name();

    let title = root
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "Composition")
        .and_then(|c| {
            c.children()
                .find(|n| n.is_element() && n.tag_name().name() == "title")
                .and_then(|t| t.attribute("value"))
        })
        .map_or_else(|| crate::words::text("medical.fhir.default-title-xml"), str::to_string);

    let mut md = String::new();
    let _ = writeln!(md, "---\ntitle: {}\nfhir_resource_type: {root_name}\n---\n", yaml_escape(&title));
    let _ = writeln!(md, "# {title}\n");
    let _ = writeln!(md, "## {}\n", crate::words::text("medical.fhir-xml.resources-heading"));
    let _ = writeln!(md, "{}", crate::words::text("medical.fhir-xml.resources-head"));
    let _ = writeln!(md, "| --- | --- | --- |");

    let mut idx = 0usize;
    for entry in root
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "entry")
    {
        if let Some(res_container) = entry
            .children()
            .find(|n| n.is_element() && n.tag_name().name() == "resource")
        {
            for res in res_container.children().filter(|n| n.is_element()) {
                idx += 1;
                let id = res
                    .children()
                    .find(|n| n.is_element() && n.tag_name().name() == "id")
                    .and_then(|n| n.attribute("value"))
                    .unwrap_or("-");
                let _ = writeln!(md, "| {idx} | {} | {} |", res.tag_name().name(), md_cell(id));
            }
        }
    }
    Ok(md)
}

fn render_hl7v2_to_markdown(text: &str) -> String {
    let mut md = String::new();
    let mut msg_type = String::from("-");
    let mut msg_ctrl = String::from("-");
    let mut segments: Vec<(&str, Vec<&str>)> = Vec::new();

    for raw_line in text.split(['\r', '\n']) {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        let seg = fields.first().copied().unwrap_or("");
        if seg == "MSH" {
            msg_type = fields.get(8).copied().unwrap_or("-").to_string();
            msg_ctrl = fields.get(9).copied().unwrap_or("-").to_string();
        }
        segments.push((seg, fields));
    }

    let _ = writeln!(md, "---\ntitle: {}\nhl7_message_type: {}\n---\n", yaml_escape(&crate::words::text("medical.hl7v2.title")), yaml_escape(&msg_type));
    let _ = writeln!(md, "# {}\n", crate::words::format("medical.hl7v2.heading", &[("type", &msg_type)]));
    let _ = writeln!(md, "- {}", crate::words::format("medical.hl7v2.control-id", &[("id", &msg_ctrl)]));
    let _ = writeln!(md, "- {}\n", crate::words::format("medical.hl7v2.segment-count", &[("count", &segments.len())]));
    let _ = writeln!(md, "## {}\n", crate::words::text("medical.hl7v2.segments-heading"));
    let _ = writeln!(md, "{}", crate::words::text("medical.hl7v2.segments-head"));
    let _ = writeln!(md, "| --- | --- | --- |");
    for (i, (seg, fields)) in segments.iter().enumerate() {
        let preview = fields
            .iter()
            .skip(1)
            .take(6)
            .copied()
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = writeln!(md, "| {} | `{seg}` | {} |", i + 1, md_cell(&preview));
    }
    md
}

fn res_type(v: &Value) -> &str {
    v.get("resourceType").and_then(Value::as_str).unwrap_or("")
}

fn extract_human_name(v: &Value) -> String {
    if let Some(arr) = v.get("name").and_then(Value::as_array)
        && let Some(first) = arr.first()
    {
        if let Some(t) = first.get("text").and_then(Value::as_str) {
            return t.to_string();
        }
        let fam = first.get("family").and_then(Value::as_str).unwrap_or("");
        let given = first
            .get("given")
            .and_then(Value::as_array)
            .and_then(|g| g.first())
            .and_then(Value::as_str)
            .unwrap_or("");
        let joined = format!("{fam} {given}").trim().to_string();
        if !joined.is_empty() {
            return joined;
        }
    }
    "-".to_string()
}

fn extract_codeable(v: Option<&Value>) -> (String, String) {
    let Some(v) = v else {
        return ("-".to_string(), "-".to_string());
    };
    let text = v.get("text").and_then(Value::as_str);
    let coding = v
        .get("coding")
        .and_then(Value::as_array)
        .and_then(|a| a.first());
    let disp = text
        .or_else(|| coding.and_then(|c| c.get("display").and_then(Value::as_str)))
        .unwrap_or("-")
        .to_string();
    let code = coding
        .and_then(|c| c.get("code").and_then(Value::as_str))
        .unwrap_or("-")
        .to_string();
    (disp, code)
}

fn extract_observation_value(o: &Value) -> String {
    if let Some(vq) = o.get("valueQuantity") {
        let val = vq
            .get("value")
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        let unit = vq.get("unit").and_then(Value::as_str).unwrap_or("");
        return format!("{val} {unit}").trim().to_string();
    }
    if let Some(vs) = o.get("valueString").and_then(Value::as_str) {
        return vs.to_string();
    }
    if let Some(vc) = o.get("valueCodeableConcept") {
        return extract_codeable(Some(vc)).0;
    }
    "-".to_string()
}

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace(['\r', '\n'], " ")
}

fn yaml_escape(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC_FHIR_BUNDLE: &str = include_str!("../tests/fixtures/synthetic_fhir_3doc6info.json");

    #[test]
    fn converts_fhir_3doc6info_bundle_to_markdown_and_docx_model() {
        let md = input_to_markdown(SYNTHETIC_FHIR_BUNDLE.as_bytes())
            .expect("fhir parse")
            .expect("should detect FHIR Bundle");
        assert!(md.contains("# 診療情報提供書（テストサンプル）"));
        assert!(md.contains("山田 太郎"));
        assert!(md.contains("2型糖尿病"));
        assert!(md.contains("ペニシリン"));
        assert!(md.contains("6.8 %"));
        assert!(md.contains("メトホルミン塩酸塩錠250mg"));

        let opened = crate::markdown::open_markdown(md.into_bytes()).expect("open_markdown");
        assert!(opened.presentation.flow.is_some());
    }

    #[test]
    fn converts_fhir_xml_and_hl7v2() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Bundle xmlns="http://hl7.org/fhir">
  <entry>
    <resource>
      <Composition>
        <id value="comp-1"/>
        <title value="退院時サマリー"/>
      </Composition>
    </resource>
  </entry>
</Bundle>"#;
        let md_xml = input_to_markdown(xml.as_bytes()).unwrap().unwrap();
        assert!(md_xml.contains("# 退院時サマリー"));
        assert!(md_xml.contains("| 1 | Composition | comp-1 |"));

        let hl7 = "MSH|^~\\&|HIS|HOSP|LAB|HOSP|202611161200||ORU^R01|MSG001|P|2.5\rPID|1||PAT001||YAMADA^TARO\rOBX|1|NM|GLU||98|mg/dL\r";
        let md_v2 = input_to_markdown(hl7.as_bytes()).unwrap().unwrap();
        assert!(md_v2.contains("ORU^R01"));
        assert!(md_v2.contains("`PID`"));
        assert!(md_v2.contains("`OBX`"));
    }

    #[test]
    fn ignores_non_fhir_json_and_plain_text() {
        assert_eq!(input_to_markdown(b"{\"hello\": \"world\"}").unwrap(), None);
        assert_eq!(input_to_markdown(b"# Normal Markdown\nhello").unwrap(), None);
    }

    #[test]
    fn converts_tokutei_kenshin_cda_and_roundtrips_docx_to_fhir_bundle() {
        let cda = r#"<?xml version="1.0" encoding="UTF-8"?>
<ClinicalDocument xmlns="urn:hl7-org:v3">
  <title>特定健康診査結果報告書</title>
  <effectiveTime value="20261115"/>
  <component><observation>
    <code code="9N001000000000001" displayName="腹囲"/>
    <value value="82.5" unit="cm"/>
  </observation></component>
</ClinicalDocument>"#;
        let md = input_to_markdown(cda.as_bytes()).unwrap().unwrap();
        assert!(md.contains("特定健康診査結果報告書"));
        assert!(md.contains("腹囲"));
        assert!(md.contains("82.5"));

        // Roundtrip FHIR JSON -> DOCX FlowDocument -> FHIR Bundle JSON
        let fhir_md = input_to_markdown(SYNTHETIC_FHIR_BUNDLE.as_bytes()).unwrap().unwrap();
        let opened = crate::markdown::open_markdown(fhir_md.into_bytes()).unwrap();
        let bundle_json = flow_to_fhir_bundle_json(&opened.presentation).expect("flow_to_fhir_bundle_json");
        assert!(bundle_json.contains("\"resourceType\": \"Bundle\""));
        assert!(bundle_json.contains("山田 太郎"));
        assert!(bundle_json.contains("2型糖尿病"));
        assert!(bundle_json.contains("HbA1c(NGSP)"));
    }
}

/// The text runs of a paragraph, joined.
fn flow_paragraph_plain_text(p: &ibis_doc_model::FlowParagraph) -> String {
    let mut out = String::new();
    for r in &p.runs {
        if let ibis_doc_model::FlowRunKind::Text(t) = &r.kind {
            out.push_str(t);
        }
    }
    out
}
