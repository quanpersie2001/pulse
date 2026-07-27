//! Documentation section extraction scenarios.
//!
//! These are pure unit tests over bytes plus a [`DocumentRecord`]: extraction
//! has no disk dependency. They cover the test-matrix rows R8-R17 that are
//! owned by the extraction contract:
//!
//! - R8  UTF-8 LF extraction: exact heading path / range / hashes.
//! - R9  CRLF: same logical refs/ranges; byte hashes reflect CRLF.
//! - R10 preamble: stable `#preamble` when meaningful; none for whitespace.
//! - R11 duplicate headings: `errors`, `errors-2`, `errors-3`.
//! - R12 fenced code containing `#`: no false heading, no split inside fence.
//! - R13 nested headings: correct hierarchy and non-duplicated base bodies.
//! - R14 empty heading + multiple H1: stable fallback + warning, no panic.
//! - R15 oversized section: safe chunks, stable `@N` refs, no fence split.
//! - R16 determinism: same bytes produce identical refs/hashes twice.
//! - R17 heading rename produces different anchors (stale-ref prerequisite).

use pulse::canonical_json::hash_bytes;
use pulse::docs::model::{
    DocumentAuthority, DocumentKind, DocumentLifecycle, DocumentRecord, DocumentScope,
    RetrievalConfig, ReviewPolicy,
};
use pulse::docs::{
    anchor_for_heading, extract_document_title, extract_sections, ExtractionWarning, TitleSource,
};

/// An approved / current / domain document used by every scenario.
fn doc() -> DocumentRecord {
    DocumentRecord {
        id: "DOC-AUTH-DOMAIN".to_string(),
        revision: 3,
        path: "docs/domain/token-lifecycle.md".to_string(),
        kind: DocumentKind::Domain,
        authority: DocumentAuthority::Approved,
        lifecycle: DocumentLifecycle::Current,
        owner: "team:identity".to_string(),
        summary: "Token types, lifecycle transitions and invariants.".to_string(),
        aliases: vec!["refresh tokens".to_string()],
        scope: DocumentScope {
            domains: vec!["authentication".to_string()],
            ..Default::default()
        },
        review_policy: ReviewPolicy::Independent,
        verification_profile: "domain-doc".to_string(),
        generated: None,
        superseded_by: None,
        retrieval: None,
    }
}

/// Run extraction against bytes, passing the exact content hash of those bytes.
fn extract(bytes: &[u8]) -> Vec<pulse::docs::SectionRecord> {
    let config = RetrievalConfig::defaults();
    let outcome = extract_sections(
        &doc(),
        doc().revision,
        &hash_bytes(bytes),
        bytes,
        &config,
        true,
    );
    outcome.sections
}

fn extract_outcome(bytes: &[u8]) -> pulse::docs::ExtractionOutcome {
    let config = RetrievalConfig::defaults();
    extract_sections(
        &doc(),
        doc().revision,
        &hash_bytes(bytes),
        bytes,
        &config,
        true,
    )
}

/// Mirror the parser's byte slicing for a 1-based inclusive line range, so tests
/// can independently recompute the expected section content hash. Each segment
/// owns its trailing line terminator (LF or CRLF), matching the extractor.
fn segments(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            out.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

fn expected_section_hash(bytes: &[u8], start_line: u32, end_line: u32) -> String {
    let mut buf = Vec::new();
    for seg in &segments(bytes)[(start_line - 1) as usize..end_line as usize] {
        buf.extend_from_slice(seg);
    }
    hash_bytes(&buf)
}

fn by_anchor<'a>(
    sections: &'a [pulse::docs::SectionRecord],
    anchor: &str,
) -> &'a pulse::docs::SectionRecord {
    sections
        .iter()
        .find(|s| s.anchor == anchor)
        .unwrap_or_else(|| panic!("missing section with anchor {anchor:?}"))
}

// ---------------------------------------------------------------------------
// R8 — UTF-8 LF extraction
// ---------------------------------------------------------------------------

#[test]
fn r8_utf8_lf_exact_paths_ranges_hashes() {
    let text = "# Token Lifecycle\n\
                \n\
                Intro before subsection.\n\
                \n\
                ## Refresh token lifecycle\n\
                \n\
                Refresh tokens rotate on use.\n\
                \n\
                ## Expired tokens\n\
                \n\
                Expired tokens cannot be refreshed.\n";
    let bytes = text.as_bytes();
    let sections = extract(bytes);

    // No preamble (first line is a heading).
    assert_eq!(sections.len(), 3);
    assert!(sections
        .iter()
        .all(|s| s.document_content_hash == hash_bytes(bytes)));

    let title = by_anchor(&sections, "token-lifecycle");
    assert_eq!(title.heading, "Token Lifecycle");
    assert_eq!(title.heading_path, vec!["Token Lifecycle"]);
    assert_eq!(title.range.start_line, 1);
    assert_eq!(title.range.end_line, 11);
    assert_eq!(title.ordinal, 0);
    assert_eq!(title.chunk, None);
    assert_eq!(
        title.section_content_hash,
        expected_section_hash(bytes, 1, 11)
    );

    let refresh = by_anchor(&sections, "refresh-token-lifecycle");
    assert_eq!(
        refresh.heading_path,
        vec!["Token Lifecycle", "Refresh token lifecycle"]
    );
    assert_eq!(refresh.range.start_line, 5);
    assert_eq!(refresh.range.end_line, 8);
    assert_eq!(refresh.ordinal, 1);
    assert_eq!(
        refresh.section_ref,
        "DOC-AUTH-DOMAIN#refresh-token-lifecycle"
    );
    assert_eq!(
        refresh.section_content_hash,
        expected_section_hash(bytes, 5, 8)
    );

    let expired = by_anchor(&sections, "expired-tokens");
    assert_eq!(
        expired.heading_path,
        vec!["Token Lifecycle", "Expired tokens"]
    );
    assert_eq!(expired.range.start_line, 9);
    assert_eq!(expired.range.end_line, 11);
    assert_eq!(
        expired.section_content_hash,
        expected_section_hash(bytes, 9, 11)
    );

    // Document-level metadata is carried on every record.
    assert!(sections
        .iter()
        .all(|s| s.document_title == "Token Lifecycle"));
    assert!(sections.iter().all(|s| s.authority == "approved"));
    assert!(sections.iter().all(|s| s.lifecycle == "current"));
    assert!(sections.iter().all(|s| s.kind == "domain"));
    assert!(sections.iter().all(|s| s.owner == "team:identity"));
    assert!(sections.iter().all(|s| s.domains == vec!["authentication"]));
    assert!(sections.iter().all(|s| s.aliases == vec!["refresh tokens"]));
    assert!(sections.iter().all(|s| s.body_indexed));
    assert!(sections.iter().all(|s| s.document_revision == 3));
}

#[test]
fn r8_title_resolves_to_first_h1() {
    let (title, source) = extract_document_title("# Real Title\nbody\n");
    assert_eq!(title, "Real Title");
    assert_eq!(source, TitleSource::FirstH1);
}

#[test]
fn r8_title_falls_back_to_normalized_file_stem_when_h1_missing() {
    let outcome = extract_outcome(b"## Only H2\nbody\n");
    assert_eq!(outcome.sections[0].document_title, "Token Lifecycle");
    assert!(outcome
        .warnings
        .iter()
        .any(|w| matches!(w, ExtractionWarning::MissingTitleFallback)));
}

#[test]
fn r8_section_hash_includes_bom_for_line_one_slice() {
    let bytes = b"\xEF\xBB\xBF# Title\n\nBody\n";
    let sections = extract(bytes);
    let title = by_anchor(&sections, "title");
    assert_eq!(title.range, pulse::docs::SectionRange::new(1, 3));
    assert_eq!(
        title.section_content_hash,
        expected_section_hash(bytes, 1, 3)
    );
}

// ---------------------------------------------------------------------------
// R9 — CRLF produces the same logical refs/ranges; byte hashes differ
// ---------------------------------------------------------------------------

#[test]
fn r9_crlf_same_logical_refs_different_byte_hashes() {
    let lf = "# Title\n\nIntro.\n\n## Sub\n\nBody.\n";
    let crlf: Vec<u8> = lf
        .as_bytes()
        .iter()
        .flat_map(|&b| {
            if b == b'\n' {
                vec![b'\r', b'\n']
            } else {
                vec![b]
            }
        })
        .collect();

    let lf_sections = extract(lf.as_bytes());
    let crlf_sections = extract(&crlf);

    assert_eq!(lf_sections.len(), crlf_sections.len());
    for (lf_sec, crlf_sec) in lf_sections.iter().zip(crlf_sections.iter()) {
        assert_eq!(lf_sec.anchor, crlf_sec.anchor, "anchor must match");
        assert_eq!(
            lf_sec.heading_path, crlf_sec.heading_path,
            "heading_path must match"
        );
        assert_eq!(lf_sec.range, crlf_sec.range, "logical range must match");
        assert_eq!(lf_sec.section_ref, crlf_sec.section_ref, "ref must match");
    }

    // Byte hashes reflect the different terminators.
    assert_ne!(
        lf_sections[0].document_content_hash,
        crlf_sections[0].document_content_hash
    );
    assert_ne!(
        lf_sections[0].section_content_hash,
        crlf_sections[0].section_content_hash
    );

    // CRLF section hashes match the CRLF byte slices exactly.
    let title = by_anchor(&crlf_sections, "title");
    assert_eq!(title.range, pulse::docs::SectionRange::new(1, 7));
    assert_eq!(
        title.section_content_hash,
        expected_section_hash(&crlf, 1, 7)
    );
    let sub = by_anchor(&crlf_sections, "sub");
    assert_eq!(sub.range, pulse::docs::SectionRange::new(5, 7));
    assert_eq!(sub.section_content_hash, expected_section_hash(&crlf, 5, 7));
}

// ---------------------------------------------------------------------------
// R10 — preamble
// ---------------------------------------------------------------------------

#[test]
fn r10_preamble_when_meaningful_content_precedes_first_heading() {
    let text = "This is preamble content.\n\
                \n\
                # Heading One\n\
                body\n";
    let bytes = text.as_bytes();
    let sections = extract(bytes);

    assert_eq!(sections.len(), 2);
    let preamble = by_anchor(&sections, "preamble");
    assert_eq!(preamble.heading, "Preamble");
    assert_eq!(preamble.heading_path, vec!["Preamble"]);
    assert_eq!(preamble.range.start_line, 1);
    assert_eq!(preamble.range.end_line, 2);
    assert_eq!(preamble.ordinal, 0);
    assert_eq!(
        preamble.section_content_hash,
        expected_section_hash(bytes, 1, 2)
    );

    let h1 = by_anchor(&sections, "heading-one");
    assert_eq!(h1.ordinal, 1);
    assert_eq!(h1.range.start_line, 3);
    assert_eq!(h1.range.end_line, 4);
}

#[test]
fn r10_no_preamble_for_whitespace_only_prefix() {
    // Only whitespace before the first heading: no preamble section.
    let text = "   \n\
                \n\
                # Only\n\
                body\n";
    let sections = extract(text.as_bytes());
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].anchor, "only");
    assert_eq!(sections[0].range.start_line, 3);
}

// ---------------------------------------------------------------------------
// R11 — duplicate headings get deterministic suffixes
// ---------------------------------------------------------------------------

#[test]
fn r11_duplicate_headings_get_suffixes() {
    let text = "# Errors\n\
                a\n\
                ## Errors\n\
                b\n\
                # errors\n\
                c\n";
    let sections = extract(text.as_bytes());
    let anchors: Vec<&str> = sections.iter().map(|s| s.anchor.as_str()).collect();
    assert_eq!(anchors, vec!["errors", "errors-2", "errors-3"]);
    let refs: Vec<&str> = sections.iter().map(|s| s.section_ref.as_str()).collect();
    assert_eq!(
        refs,
        vec![
            "DOC-AUTH-DOMAIN#errors",
            "DOC-AUTH-DOMAIN#errors-2",
            "DOC-AUTH-DOMAIN#errors-3",
        ]
    );
}

// ---------------------------------------------------------------------------
// R12 — fenced code containing '#': no false heading, fence stays atomic
// ---------------------------------------------------------------------------

#[test]
fn r12_fenced_code_hash_is_not_a_heading() {
    let text = "# Real Heading\n\
                \n\
                ```rust\n\
                # not a heading\n\
                let x = \"# also not\";\n\
                ```\n\
                \n\
                ## Sub\n\
                text\n";
    let sections = extract(text.as_bytes());

    // Only the two real headings become sections.
    assert_eq!(sections.len(), 2);
    let anchors: Vec<&str> = sections.iter().map(|s| s.anchor.as_str()).collect();
    assert_eq!(anchors, vec!["real-heading", "sub"]);

    // No section derives from the in-fence '#' lines.
    assert!(sections
        .iter()
        .all(|s| !s.heading.contains("not a heading")));
}

// ---------------------------------------------------------------------------
// R13 — nested headings: correct hierarchy, non-duplicated base bodies
// ---------------------------------------------------------------------------

#[test]
fn r13_nested_headings_hierarchy_and_distinct_bodies() {
    let text = "# Title\n\
                intro\n\
                ## Child A\n\
                a body\n\
                ### Grandchild\n\
                grand body\n\
                ## Child B\n\
                b body\n\
                # Title2\n\
                t2 body\n";
    let bytes = text.as_bytes();
    let sections = extract(bytes);

    assert_eq!(sections.len(), 5);

    let title = by_anchor(&sections, "title");
    assert_eq!(title.heading_path, vec!["Title"]);
    assert_eq!(title.range, pulse::docs::SectionRange::new(1, 8));

    let child_a = by_anchor(&sections, "child-a");
    assert_eq!(child_a.heading_path, vec!["Title", "Child A"]);
    assert_eq!(child_a.range, pulse::docs::SectionRange::new(3, 6));

    let grand = by_anchor(&sections, "grandchild");
    assert_eq!(grand.heading_path, vec!["Title", "Child A", "Grandchild"]);
    assert_eq!(grand.range, pulse::docs::SectionRange::new(5, 6));

    let child_b = by_anchor(&sections, "child-b");
    assert_eq!(child_b.heading_path, vec!["Title", "Child B"]);
    assert_eq!(child_b.range, pulse::docs::SectionRange::new(7, 8));

    let title2 = by_anchor(&sections, "title2");
    assert_eq!(title2.heading_path, vec!["Title2"]);
    assert_eq!(title2.range, pulse::docs::SectionRange::new(9, 10));

    // Non-duplicated base bodies: every section has a distinct content hash and
    // a distinct section ref (no record copies another record's body).
    let mut hashes: Vec<&String> = sections.iter().map(|s| &s.section_content_hash).collect();
    hashes.sort();
    hashes.dedup();
    assert_eq!(
        hashes.len(),
        sections.len(),
        "section content hashes must be unique"
    );
    let mut refs: Vec<&String> = sections.iter().map(|s| &s.section_ref).collect();
    refs.sort();
    refs.dedup();
    assert_eq!(refs.len(), sections.len(), "section refs must be unique");

    // The grandchild range is a proper sub-slice of Child A and must hash differently.
    assert_ne!(grand.section_content_hash, child_a.section_content_hash);
}

// ---------------------------------------------------------------------------
// R14 — empty heading + multiple H1: stable fallback + warnings, no panic
// ---------------------------------------------------------------------------

#[test]
fn r14_empty_heading_and_multiple_h1_warn_without_panic() {
    let text = "# First Title\n\
                \n\
                ##\n\
                after empty\n\
                # Second Title\n\
                body\n";
    let bytes = text.as_bytes();
    let outcome = extract_outcome(bytes);

    // First H1 still wins as the document title; every record carries it.
    assert!(outcome
        .sections
        .iter()
        .all(|s| s.document_title == "First Title"));

    // Multiple H1 warning with the exact count.
    let multi = outcome
        .warnings
        .iter()
        .filter(|w| matches!(w, ExtractionWarning::MultipleDocumentTitles { .. }))
        .count();
    assert_eq!(multi, 1);
    match outcome
        .warnings
        .iter()
        .find(|w| matches!(w, ExtractionWarning::MultipleDocumentTitles { .. }))
    {
        Some(ExtractionWarning::MultipleDocumentTitles { count }) => assert_eq!(*count, 2),
        other => panic!("expected MultipleDocumentTitles, got {other:?}"),
    }

    // The empty heading still gets an outline record with the fallback anchor.
    let empty = by_anchor(&outcome.sections, "section");
    assert_eq!(empty.heading, "");
    assert_eq!(empty.range.start_line, 3);
    let empty_warn = outcome
        .warnings
        .iter()
        .filter(|w| matches!(w, ExtractionWarning::EmptyHeading { .. }))
        .count();
    assert_eq!(empty_warn, 1);
    assert!(outcome.warnings.iter().any(|w| matches!(
        w,
        ExtractionWarning::EmptyHeading { section_ref } if section_ref == "DOC-AUTH-DOMAIN#section"
    )));

    // No missing-title warning because an H1 exists.
    assert!(!outcome
        .warnings
        .iter()
        .any(|w| matches!(w, ExtractionWarning::MissingTitleFallback)));

    // The two real H1s keep distinct anchors.
    assert!(outcome.sections.iter().any(|s| s.anchor == "first-title"));
    assert!(outcome.sections.iter().any(|s| s.anchor == "second-title"));
}

// ---------------------------------------------------------------------------
// R15 — oversized section: safe chunks, stable @N refs, no fence split
// ---------------------------------------------------------------------------

#[test]
fn r15_oversized_section_chunks_without_splitting_fence() {
    // One heading, an intro line, a fenced block large enough that it cannot be
    // split (180 fence lines > 160-line soft cap), then trailing filler.
    let mut text = String::from("# Big Section\nintro\n```\n");
    for i in 0..178 {
        text.push_str(&format!("code line {i}\n"));
    }
    text.push_str("```\n");
    for i in 0..28 {
        text.push_str(&format!("filler {i}\n"));
    }
    let bytes = text.as_bytes();

    let sections = extract(bytes);

    // Oversized -> chunked. All records share the base anchor and carry a chunk.
    assert!(
        sections.len() >= 2,
        "oversized section must be split into multiple chunks"
    );
    assert!(sections.iter().all(|s| s.anchor == "big-section"));
    assert!(sections.iter().all(|s| s.chunk.is_some()));

    // Stable, consecutive @N refs with a consistent total.
    let total = sections.len() as u32;
    assert!(sections.iter().all(|s| s.chunk.unwrap().total == total));
    let mut ordinals: Vec<u32> = sections.iter().map(|s| s.chunk.unwrap().ordinal).collect();
    ordinals.sort();
    assert_eq!(ordinals, (1..=total).collect::<Vec<u32>>());
    let refs: Vec<String> = (1..=total)
        .map(|n| format!("DOC-AUTH-DOMAIN#big-section@{n}"))
        .collect();
    let got: Vec<String> = sections.iter().map(|s| s.section_ref.clone()).collect();
    assert_eq!(got, refs);

    // Every chunk stays within the section bounds.
    let last_line = segments(bytes).len() as u32;
    assert!(sections
        .iter()
        .all(|s| s.range.start_line >= 1 && s.range.end_line <= last_line));

    // The fenced block (opens at line 3, closes at line 182) must be fully
    // contained in a single chunk — i.e., never split across chunks. That chunk
    // necessarily exceeds the 160-line soft cap, proving the fence stayed atomic.
    let fence_open = 3u32;
    let fence_close = 182u32;
    let containing: Vec<&pulse::docs::SectionRecord> = sections
        .iter()
        .filter(|s| s.range.start_line <= fence_open && s.range.end_line >= fence_close)
        .collect();
    assert_eq!(
        containing.len(),
        1,
        "fenced block must live in exactly one chunk"
    );
    let fence_chunk = containing[0];
    assert!(
        fence_chunk.range.line_count() >= 180,
        "atomic fence chunk must not be split (got {} lines)",
        fence_chunk.range.line_count()
    );

    // Each chunk's section hash matches its exact byte slice.
    for s in &sections {
        assert_eq!(
            s.section_content_hash,
            expected_section_hash(bytes, s.range.start_line, s.range.end_line)
        );
    }
}

// ---------------------------------------------------------------------------
// R16 — determinism: same bytes produce identical refs/hashes twice
// ---------------------------------------------------------------------------

#[test]
fn r16_same_bytes_produce_identical_extraction() {
    let text = "# Title\n\nintro\n\n## Sub\n\nbody\n";
    let bytes = text.as_bytes();
    let a = extract_outcome(bytes);
    let b = extract_outcome(bytes);
    assert_eq!(a, b, "extraction must be deterministic for identical bytes");

    // Spot-check that the section hashes are real sha256 values and stable.
    assert!(a
        .sections
        .iter()
        .all(|s| s.section_content_hash.starts_with("sha256:")));
    assert!(a
        .sections
        .iter()
        .all(|s| s.document_content_hash.starts_with("sha256:")));
}

// ---------------------------------------------------------------------------
// R17 — heading rename produces different anchors (stale-ref prerequisite)
// ---------------------------------------------------------------------------

#[test]
fn r17_heading_rename_changes_anchor() {
    assert_ne!(
        anchor_for_heading("Old Name"),
        anchor_for_heading("New Name")
    );
    assert_eq!(anchor_for_heading("Old Name"), "old-name");
    assert_eq!(anchor_for_heading("New Name"), "new-name");

    // Two documents differing only by a heading produce disjoint section refs.
    let old = extract("# Old Name\nbody\n".as_bytes());
    let new = extract("# New Name\nbody\n".as_bytes());
    assert_eq!(old[0].section_ref, "DOC-AUTH-DOMAIN#old-name");
    assert_eq!(new[0].section_ref, "DOC-AUTH-DOMAIN#new-name");
    assert!(!new
        .iter()
        .any(|s| s.section_ref == "DOC-AUTH-DOMAIN#old-name"));
}

// ---------------------------------------------------------------------------
// Bonus: serde contract — deny_unknown_fields, round-trip, no floats
// ---------------------------------------------------------------------------

#[test]
fn section_record_round_trips_through_serde() {
    let bytes = "# Title\n\nbody\n".as_bytes();
    let sections = extract(bytes);
    let record = &sections[0];
    let json = serde_json::to_string(record).expect("serialize");
    // No floats and no unknown fields survive a round trip.
    let parsed: pulse::docs::SectionRecord = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed, *record);
    // Rejecting unknown fields proves deny_unknown_fields is active.
    let mut tampered = serde_json::from_str::<serde_json::Value>(&json).unwrap();
    tampered
        .as_object_mut()
        .unwrap()
        .insert("unknown_field".to_string(), serde_json::Value::Bool(true));
    let tampered_json = serde_json::to_string(&tampered).unwrap();
    assert!(serde_json::from_str::<pulse::docs::SectionRecord>(&tampered_json).is_err());
}
