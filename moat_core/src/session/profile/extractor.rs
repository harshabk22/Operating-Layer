use super::store::ProfileStore;
use crate::session::MessageRecord;
use anyhow::Result;
use regex::Regex;
use std::sync::LazyLock;

static PROJECT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:my project|the project|project name)\s+(?:is\s+)?(?:called\s+|named\s+)?["']?([A-Za-z][A-Za-z0-9_-]{1,30})["']?"#).unwrap()
});

static CORRECTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)\b(no[,.]?\s+|wrong[,.]?\s+|actually[,.]?\s+|that.s wrong[,.]?\s+)(.{10,100})"#,
    )
    .unwrap()
});

static LANGUAGE_PREF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(I use|I prefer|we use|we're using|built with|written in)\s+(Rust|Python|TypeScript|JavaScript|Go|Java|C\+\+|Swift|Kotlin)\b"#).unwrap()
});

pub struct ProfileExtractor;

impl ProfileExtractor {
    pub fn extract_and_store(
        messages: &[MessageRecord],
        session_id: &str,
        profile: &ProfileStore,
    ) -> Result<Vec<String>> {
        let mut learned = Vec::new();

        for msg in messages {
            if msg.role == "user" {
                if let Some(caps) = PROJECT_NAME_RE.captures(&msg.content) {
                    if let Some(name) = caps.get(1) {
                        let name = name.as_str().trim().to_string();
                        profile.upsert_preference("project_name", &name, 0.85, session_id)?;
                        learned.push(format!("project_name = {}", name));
                    }
                }

                if let Some(caps) = LANGUAGE_PREF_RE.captures(&msg.content) {
                    if let Some(lang) = caps.get(2) {
                        let lang = lang.as_str().to_string();
                        profile.upsert_preference("preferred_language", &lang, 0.80, session_id)?;
                        learned.push(format!("preferred_language = {}", lang));
                    }
                }

                if let Some(caps) = CORRECTION_RE.captures(&msg.content) {
                    if let Some(correction_text) = caps.get(2) {
                        profile.record_correction(
                            &msg.content,
                            correction_text.as_str(),
                            session_id,
                        )?;
                        learned.push("correction recorded".into());
                    }
                }
            }
        }

        if !learned.is_empty() {
            println!("[ProfileExtractor] Learned: {:?}", learned);
        }

        Ok(learned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_name_extracts_correctly() {
        let text = "My project is called Ageex and I use Rust";
        let caps = PROJECT_NAME_RE.captures(text).unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "Ageex");
    }
}
