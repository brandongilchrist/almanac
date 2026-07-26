//! Manifest emission: best-effort parse of agent output → [`Manifest`] list.
//!
//! Per `10_PLAN.md`:
//! - Each `Manifest` has `manifest_id = "<run_id>:<schema_id>"`.
//! - `materialized_at` = now.
//! - `content_hash` = SHA-256 hex of the agent's output content.
//!
//! If the agent output contains parseable artifact references (GitHub commit
//! URLs, file paths in code fences), one manifest is emitted per reference.
//! If nothing parseable is found, a single fallback manifest is emitted with
//! `schema_id = "agent-output"` and `uri` = the run's Buzz thread URL.

use crate::error::IngestError;
use crate::model::Manifest;
use crate::source::SourceEvent;
use sha2::{Digest, Sha256};

/// Regex-free extractors for artifact references. Returns `(schema_id, uri, commit_sha)`.
///
/// - GitHub commit URLs: `https://github.com/<owner>/<repo>/commit/<sha>`
///   → schema_id `github-commit`, commit_sha = the 40-hex sha.
/// - GitHub blob URLs: `https://github.com/<owner>/<repo>/blob/<sha>/...`
///   → schema_id `github-blob`, commit_sha = the sha segment.
/// - File paths in code fences: `` `path/to/file.ext` ``
///   → schema_id `file`, uri = the path.
fn extract_refs(content: &str) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();

    for line in content.lines() {
        // GitHub commit URLs.
        if let Some(rest) = line.find("github.com/") {
            let after = &line[rest + "github.com/".len()..];
            if let Some(cidx) = after.find("/commit/") {
                let sha_area = &after[cidx + "/commit/".len()..];
                let sha: String = sha_area
                    .chars()
                    .take_while(|c| c.is_ascii_hexdigit())
                    .collect();
                if sha.len() >= 7 {
                    let owner_repo: String = after[..cidx].to_string();
                    let uri = format!("https://github.com/{owner_repo}/commit/{sha}");
                    out.push(("github-commit".into(), uri, Some(sha)));
                    continue;
                }
            }
            if let Some(bidx) = after.find("/blob/") {
                let after_blob = &after[bidx + "/blob/".len()..];
                // sha is the segment up to the next '/'
                let sha: String = after_blob.chars().take_while(|c| *c != '/').collect();
                if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    let owner_repo: String = after[..bidx].to_string();
                    let path_segment = &after_blob[sha.len()..];
                    let uri = format!("https://github.com/{owner_repo}/blob/{sha}{path_segment}");
                    out.push(("github-blob".into(), uri, Some(sha)));
                    continue;
                }
            }
        }

        // File paths in backticks: `src/foo.rs` or `./path/to/file`.
        let bytes = line.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == b'`' {
                if let Some(end_rel) = line[i + 1..].find('`') {
                    let end = i + 1 + end_rel;
                    let candidate = &line[i + 1..end];
                    if looks_like_path(candidate) {
                        out.push(("file".into(), candidate.to_string(), None));
                    }
                    i = end + 1;
                    continue;
                }
            }
            i += 1;
        }
    }
    out
}

/// Heuristic: does `s` look like a file path we'd want to record?
fn looks_like_path(s: &str) -> bool {
    if s.len() < 3 || s.contains(' ') || s.contains('\n') {
        return false;
    }
    // Must contain a path separator OR start with `.` OR have a known extension.
    s.contains('/') || s.starts_with("./") || s.starts_with(".\\") || has_extension(s)
}

fn has_extension(s: &str) -> bool {
    if let Some(dot) = s.rfind('.') {
        let ext = &s[dot + 1..];
        !ext.is_empty() && ext.len() <= 6 && ext.chars().all(|c| c.is_ascii_alphanumeric())
    } else {
        false
    }
}

/// SHA-256 hex of `content`.
pub fn content_hash(content: &str) -> String {
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    let bytes = h.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Build one or more [`Manifest`]s from an agent's output event.
///
/// Falls back to a single manifest pointing at the run's output thread when
/// no parseable artifact references are found.
pub fn manifest_from_agent_output(
    run_id: &str,
    event: &SourceEvent,
    now: i64,
    fallback_thread_uri: Option<&str>,
) -> Result<Vec<Manifest>, IngestError> {
    let hash = content_hash(&event.content);
    let refs = extract_refs(&event.content);

    if refs.is_empty() {
        let schema_id = "agent-output".to_string();
        let uri = fallback_thread_uri
            .map(str::to_owned)
            .unwrap_or_else(|| format!("buzz://run/{run_id}"));
        return Ok(vec![Manifest {
            manifest_id: Manifest::id_for(run_id, &schema_id),
            run_id: run_id.to_string(),
            schema_id,
            schema_version: 1,
            content_hash: hash,
            commit_sha: None,
            uri,
            bytes: Some(event.content.len() as u64),
            materialized_at: now,
        }]);
    }

    // Dedup by schema_id: a run emitting two manifests with the same schema_id
    // is a producer bug; per the spec the second overwrites the first (LWW).
    // We keep the last seen for each schema_id.
    let mut by_schema: std::collections::BTreeMap<String, Manifest> = Default::default();
    for (schema_id, uri, commit_sha) in refs {
        by_schema.insert(
            schema_id.clone(),
            Manifest {
                manifest_id: Manifest::id_for(run_id, &schema_id),
                run_id: run_id.to_string(),
                schema_id,
                schema_version: 1,
                content_hash: hash.clone(),
                commit_sha,
                uri,
                bytes: Some(event.content.len() as u64),
                materialized_at: now,
            },
        );
    }
    Ok(by_schema.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{SourceEvent, Tag};

    fn out(content: &str) -> SourceEvent {
        SourceEvent {
            kind: 0,
            content: content.into(),
            tags: Vec::<Tag>::new(),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn github_commit_url_extracts_manifest() {
        let e = out(
            "Done. See https://github.com/org/repo/commit/deadbeefcafebabe1234567890abcdef12345678",
        );
        let ms = manifest_from_agent_output("run-1", &e, 100, None).unwrap();
        assert_eq!(ms.len(), 1);
        let m = &ms[0];
        assert_eq!(m.schema_id, "github-commit");
        assert_eq!(
            m.commit_sha.as_deref(),
            Some("deadbeefcafebabe1234567890abcdef12345678")
        );
        assert_eq!(m.manifest_id, "run-1:github-commit");
        assert_eq!(m.materialized_at, 100);
        assert!(m.uri.starts_with("https://github.com/org/repo/commit/"));
    }

    #[test]
    fn blob_url_extracts_manifest_with_sha() {
        let e = out("Updated `src/lib.rs` at https://github.com/org/repo/blob/deadbeefcafebabe1234567890abcdef12345678/src/lib.rs");
        let ms = manifest_from_agent_output("run-1", &e, 100, None).unwrap();
        assert_eq!(ms.len(), 1);
        let m = &ms[0];
        assert_eq!(m.schema_id, "github-blob");
        assert_eq!(
            m.commit_sha.as_deref(),
            Some("deadbeefcafebabe1234567890abcdef12345678")
        );
    }

    #[test]
    fn file_path_in_code_fence_extracts_manifest() {
        let e = out("Wrote `src/agents/brief.rs` and `./config.toml`.");
        let ms = manifest_from_agent_output("run-1", &e, 100, None).unwrap();
        let schemas: Vec<_> = ms.iter().map(|m| m.schema_id.as_str()).collect();
        // both refs collapse under schema_id "file" → one manifest, last wins
        assert_eq!(schemas, vec!["file"]);
        let m = &ms[0];
        assert_eq!(m.manifest_id, "run-1:file");
        // last-seen wins per LWW
        assert_eq!(m.uri, "./config.toml");
    }

    #[test]
    fn fallback_when_no_refs() {
        let e = out("All done, nothing to link.");
        let ms = manifest_from_agent_output("run-1", &e, 100, Some("buzz://thread/abc")).unwrap();
        assert_eq!(ms.len(), 1);
        let m = &ms[0];
        assert_eq!(m.schema_id, "agent-output");
        assert_eq!(m.uri, "buzz://thread/abc");
        assert_eq!(m.manifest_id, "run-1:agent-output");
        assert_eq!(m.schema_version, 1);
    }

    #[test]
    fn fallback_default_uri_when_no_thread() {
        let e = out("plain text");
        let ms = manifest_from_agent_output("run-1", &e, 100, None).unwrap();
        assert_eq!(ms[0].uri, "buzz://run/run-1");
    }

    #[test]
    fn content_hash_is_stable_hex() {
        let h1 = content_hash("hello");
        let h2 = content_hash("hello");
        let h3 = content_hash("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64, "sha256 hex is 64 chars");
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
