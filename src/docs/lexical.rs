use std::cmp::Ordering;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, TermQuery};
use tantivy::schema::document::Value;
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TantivyDocument, TextFieldIndexing, TextOptions, STORED,
    STRING, TEXT,
};
use tantivy::tokenizer::{LowerCaser, RawTokenizer, RemoveLongFilter, TextAnalyzer};
use tantivy::{doc, Index, Term};

use crate::docs::section::SectionRecord;
use crate::{PulseError, PulseResult};

pub const TANTIVY_COMPAT_VERSION: &str = "tantivy-0.22-pulse-docs-v2";
pub const SNIPPET_MAX_BYTES: usize = 500;
pub const PULSE_IDENTIFIER_TOKENIZER: &str = "pulse_identifier_v1";

#[derive(Debug, Clone)]
pub struct LexicalSchema {
    pub schema: Schema,
    pub section_ref: Field,
    pub record_json: Field,
    pub heading: Field,
    pub document_title: Field,
    pub heading_path: Field,
    pub aliases: Field,
    pub domains: Field,
    pub summary: Field,
    pub path: Field,
    pub body: Field,
    pub identifiers: Field,
}

pub fn build_schema() -> LexicalSchema {
    let mut builder = Schema::builder();
    let section_ref = builder.add_text_field("section_ref", STRING | STORED);
    let record_json = builder.add_text_field("record_json", STORED);
    let heading = builder.add_text_field("heading", TEXT | STORED);
    let document_title = builder.add_text_field("document_title", TEXT | STORED);
    let heading_path = builder.add_text_field("heading_path", TEXT | STORED);
    let aliases = builder.add_text_field("aliases", TEXT | STORED);
    let domains = builder.add_text_field("domains", TEXT | STORED);
    let summary = builder.add_text_field("summary", TEXT | STORED);
    let path = builder.add_text_field("path", TEXT | STORED);
    let body = builder.add_text_field("body", TEXT | STORED);
    let identifier_options = TextOptions::default().set_stored().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(PULSE_IDENTIFIER_TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let identifiers = builder.add_text_field("identifiers", identifier_options);
    let schema = builder.build();
    LexicalSchema {
        schema,
        section_ref,
        record_json,
        heading,
        document_title,
        heading_path,
        aliases,
        domains,
        summary,
        path,
        body,
        identifiers,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LexicalHit {
    pub section_ref: String,
    pub score: f64,
    pub matched_fields: Vec<String>,
    pub section: SectionRecord,
}

pub fn open_index(tantivy_dir: &Path) -> PulseResult<Index> {
    Index::open_in_dir(tantivy_dir).map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("tantivy open failed: {e}"))
    })
}

pub fn build_index(tantivy_dir: &Path, sections: &[SectionRecord]) -> PulseResult<()> {
    build_index_with_bodies(tantivy_dir, sections, &std::collections::BTreeMap::new())
}

pub fn build_index_with_bodies(
    tantivy_dir: &Path,
    sections: &[SectionRecord],
    bodies: &std::collections::BTreeMap<String, String>,
) -> PulseResult<()> {
    if tantivy_dir.exists() {
        std::fs::remove_dir_all(tantivy_dir).map_err(|e| PulseError::io(tantivy_dir, e))?;
    }
    std::fs::create_dir_all(tantivy_dir).map_err(|e| PulseError::io(tantivy_dir, e))?;
    let fields = build_schema();
    let index = Index::create_in_dir(tantivy_dir, fields.schema.clone()).map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("tantivy create failed: {e}"))
    })?;
    register_tokenizers(&index);
    let mut writer = index.writer(50_000_000).map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("tantivy writer failed: {e}"))
    })?;
    for section in sections {
        let body_text = if section.body_indexed {
            bodies
                .get(&section.section_ref)
                .cloned()
                .unwrap_or_else(|| section.heading_path.join(" "))
        } else {
            String::new()
        };
        let record_json = serde_json::to_string(section).map_err(|e| {
            PulseError::validation("json_serialize_error", format!("section record json: {e}"))
        })?;
        let mut document = doc!(
            fields.section_ref => section.section_ref.clone(),
            fields.record_json => record_json,
            fields.heading => section.heading.clone(),
            fields.document_title => section.document_title.clone(),
            fields.heading_path => section.heading_path.join(" "),
            fields.aliases => section.aliases.join(" "),
            fields.domains => section.domains.join(" "),
            fields.summary => section.summary.clone(),
            fields.path => section.path.clone(),
            fields.body => body_text,
        );
        for identifier in identifiers_for_section(section) {
            document.add_text(fields.identifiers, identifier);
        }
        writer.add_document(document).map_err(|e| {
            PulseError::validation("docs_index_corrupt", format!("tantivy add failed: {e}"))
        })?;
    }
    writer.commit().map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("tantivy commit failed: {e}"))
    })?;
    Ok(())
}

pub fn query(tantivy_dir: &Path, terms: &[String], limit: usize) -> PulseResult<Vec<LexicalHit>> {
    let index = open_index(tantivy_dir)?;
    register_tokenizers(&index);
    let fields = fields_from_schema(index.schema())?;
    let reader = index.reader().map_err(|e| {
        PulseError::validation("docs_index_corrupt", format!("tantivy reader failed: {e}"))
    })?;
    let searcher = reader.searcher();
    let normalized = normalize_terms(terms);
    if normalized.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let query = build_plain_text_query(&fields, &normalized);
    let top_docs = searcher
        .search(&query, &TopDocs::with_limit(limit.max(1)))
        .map_err(|e| PulseError::validation("docs_index_corrupt", format!("search failed: {e}")))?;
    let mut hits = Vec::new();
    for (score, address) in top_docs {
        let retrieved: TantivyDocument = searcher.doc(address).map_err(|e| {
            PulseError::validation("docs_index_corrupt", format!("doc load failed: {e}"))
        })?;
        let Some(section_ref_value) = retrieved.get_first(fields.section_ref) else {
            continue;
        };
        let Some(section_ref) = section_ref_value.as_str().map(str::to_string) else {
            continue;
        };
        let Some(record_value) = retrieved.get_first(fields.record_json) else {
            continue;
        };
        let Some(record_json) = record_value.as_str() else {
            continue;
        };
        let section = serde_json::from_str(record_json).map_err(|e| {
            PulseError::validation(
                "docs_index_corrupt",
                format!("stored section record is invalid: {e}"),
            )
        })?;
        hits.push(LexicalHit {
            section_ref,
            score: score as f64,
            matched_fields: matched_fields(&retrieved, &fields, &normalized),
            section,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.section_ref.cmp(&b.section_ref))
    });
    hits.truncate(limit);
    Ok(hits)
}

pub fn load_section_records(sections_path: &Path) -> PulseResult<Vec<SectionRecord>> {
    let text =
        std::fs::read_to_string(sections_path).map_err(|e| PulseError::io(sections_path, e))?;
    let mut sections = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let section: SectionRecord = serde_json::from_str(line).map_err(|e| {
            PulseError::validation(
                "docs_index_corrupt",
                format!("invalid sections.jsonl line {}: {e}", idx + 1),
            )
        })?;
        sections.push(section);
    }
    Ok(sections)
}

pub fn write_sections_jsonl(path: &Path, sections: &[SectionRecord]) -> PulseResult<Vec<u8>> {
    let mut bytes = Vec::new();
    for section in sections {
        let mut line = serde_json::to_vec(section).map_err(|e| {
            PulseError::validation("json_serialize_error", format!("section record json: {e}"))
        })?;
        bytes.append(&mut line);
        bytes.push(b'\n');
    }
    std::fs::write(path, &bytes).map_err(|e| PulseError::io(path, e))?;
    Ok(bytes)
}

fn fields_from_schema(schema: Schema) -> PulseResult<LexicalSchema> {
    let get = |name: &str| -> PulseResult<Field> {
        schema.get_field(name).map_err(|e| {
            PulseError::validation(
                "docs_index_corrupt",
                format!("missing tantivy field {name}: {e}"),
            )
        })
    };
    Ok(LexicalSchema {
        section_ref: get("section_ref")?,
        record_json: get("record_json")?,
        heading: get("heading")?,
        document_title: get("document_title")?,
        heading_path: get("heading_path")?,
        aliases: get("aliases")?,
        domains: get("domains")?,
        summary: get("summary")?,
        path: get("path")?,
        body: get("body")?,
        identifiers: get("identifiers")?,
        schema,
    })
}

fn normalize_terms(terms: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for term in terms.iter().flat_map(|term| tokenize_query_text(term)) {
        if !normalized.contains(&term) {
            normalized.push(term);
        }
        if normalized.len() >= 32 {
            break;
        }
    }
    normalized
}

pub fn tokenize_query_text(input: &str) -> Vec<String> {
    input
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':') {
                ch.to_lowercase().collect::<String>()
            } else {
                " ".to_string()
            }
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .take(32)
        .map(str::to_string)
        .collect()
}

fn identifiers_for_section(section: &SectionRecord) -> Vec<String> {
    let mut out = Vec::new();
    for source in [
        section.section_ref.as_str(),
        section.path.as_str(),
        section.heading.as_str(),
        section.document_title.as_str(),
        section.summary.as_str(),
    ] {
        for token in tokenize_query_text(source) {
            if token
                .chars()
                .any(|c| matches!(c, '-' | '_' | '.' | '/' | ':') || c.is_ascii_digit())
            {
                out.push(token.clone());
                out.extend(identifier_parts(&token));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn register_tokenizers(index: &Index) {
    index.tokenizers().register(
        PULSE_IDENTIFIER_TOKENIZER,
        TextAnalyzer::builder(RawTokenizer::default())
            .filter(RemoveLongFilter::limit(128))
            .filter(LowerCaser)
            .build(),
    );
}

fn build_plain_text_query(fields: &LexicalSchema, terms: &[String]) -> Box<dyn Query> {
    let mut clauses = Vec::new();
    for term in terms {
        for (field, boost) in boosted_fields(fields, term) {
            let term_query: Box<dyn Query> = Box::new(TermQuery::new(
                Term::from_field_text(field, term),
                IndexRecordOption::WithFreqs,
            ));
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(term_query, boost)) as Box<dyn Query>,
            ));
        }
        for part in identifier_parts(term) {
            let term_query: Box<dyn Query> = Box::new(TermQuery::new(
                Term::from_field_text(fields.identifiers, &part),
                IndexRecordOption::WithFreqs,
            ));
            clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(term_query, 1.4)) as Box<dyn Query>,
            ));
        }
    }
    Box::new(BooleanQuery::new(clauses))
}

fn boosted_fields(fields: &LexicalSchema, term: &str) -> Vec<(Field, f32)> {
    let mut out = vec![
        (fields.heading, 5.0),
        (fields.document_title, 4.0),
        (fields.heading_path, 3.0),
        (fields.aliases, 3.0),
        (fields.domains, 3.0),
        (fields.summary, 2.5),
        (fields.path, 1.5),
        (fields.body, 1.0),
    ];
    let identifier_boost = if term
        .chars()
        .any(|c| matches!(c, '-' | '_' | '.' | '/' | ':') || c.is_ascii_digit())
    {
        6.0
    } else {
        2.0
    };
    out.push((fields.identifiers, identifier_boost));
    out
}

fn matched_fields(
    document: &TantivyDocument,
    fields: &LexicalSchema,
    terms: &[String],
) -> Vec<String> {
    let field_names = [
        ("heading", fields.heading),
        ("document_title", fields.document_title),
        ("heading_path", fields.heading_path),
        ("aliases", fields.aliases),
        ("domains", fields.domains),
        ("summary", fields.summary),
        ("path", fields.path),
        ("body", fields.body),
        ("identifiers", fields.identifiers),
    ];
    let mut out = Vec::new();
    for (name, field) in field_names {
        let values = document
            .get_all(field)
            .filter_map(|value| value.as_str())
            .map(str::to_lowercase)
            .collect::<Vec<_>>();
        if values.iter().any(|value| text_matches_terms(value, terms)) {
            out.push(name.to_string());
        }
    }
    out
}

fn text_matches_terms(text: &str, terms: &[String]) -> bool {
    let indexed_terms = tokenize_query_text(text);
    terms.iter().any(|term| {
        text.contains(term)
            || indexed_terms.iter().any(|indexed| indexed == term)
            || identifier_parts(term)
                .iter()
                .any(|part| indexed_terms.iter().any(|indexed| indexed == part))
    })
}

fn identifier_parts(token: &str) -> Vec<String> {
    token
        .split(['-', '_', '.', '/', ':'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}
