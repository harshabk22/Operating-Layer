use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::{MoatError, MoatResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub workflow: Vec<String>,
    pub constraints: Vec<String>,
    pub best_practices: Vec<String>,
    pub raw_markdown: String,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub required_tools: Vec<String>,
    #[serde(default)]
    pub delegation: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub schema_version: Option<u32>,
}

impl SkillDefinition {
    pub fn prompt_summary(&self) -> String {
        let mut s = format!("### Skill: {}\n", self.name);
        if !self.description.is_empty() {
            s.push_str(&format!("Description: {}\n", self.description.trim()));
        }
        if !self.triggers.is_empty() {
            s.push_str(&format!("Triggers: {}\n", self.triggers.join(", ")));
        }
        if !self.required_tools.is_empty() {
            s.push_str(&format!(
                "Required Tools: {}\n",
                self.required_tools.join(", ")
            ));
        }
        // Emit explicit operational delegation directive instead of descriptive text
        if !self.delegation.is_empty() && !self.required_tools.is_empty() {
            s.push_str("\n⚠ DELEGATION REQUIREMENT:\n");
            s.push_str("If this skill requires tools you do not possess, you MUST invoke:\n");
            for delegate_target in &self.delegation {
                s.push_str(&format!(
                    "  action: \"system:spawn_worker\"\n  target: \"{}\"\n",
                    delegate_target
                ));
            }
            s.push_str("Do not explain inability. Do not stop. Delegate instead.\n\n");
        } else if !self.delegation.is_empty() {
            s.push_str(&format!(
                "Delegation Rules: {}\n",
                self.delegation.join(", ")
            ));
        }
        if !self.workflow.is_empty() {
            s.push_str("Workflow:\n");
            for step in &self.workflow {
                s.push_str(&format!("- {}\n", step));
            }
        }
        if !self.constraints.is_empty() {
            s.push_str("Constraints:\n");
            for constraint in &self.constraints {
                s.push_str(&format!("- {}\n", constraint));
            }
        }
        s
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub required_tools: Vec<String>,
    #[serde(default)]
    pub delegation: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub schema_version: Option<u32>,
}

fn split_frontmatter_local(markdown: &str) -> Option<(String, &str)> {
    let rest = if markdown.starts_with("---\n") {
        markdown.strip_prefix("---\n")?
    } else if markdown.starts_with("---\r\n") {
        markdown.strip_prefix("---\r\n")?
    } else {
        return None;
    };
    let end = rest.find("\n---\n").or_else(|| rest.find("\r\n---\r\n"))?;
    let yaml = rest[..end].to_string();
    let body = if rest[end..].starts_with("\r\n---\r\n") {
        &rest[end + 7..]
    } else {
        &rest[end + 5..]
    };
    Some((yaml, body))
}

pub fn parse_skill_file(markdown: &str) -> Option<SkillDefinition> {
    let mut name = String::new();
    let mut description = String::new();
    let mut triggers = Vec::new();
    let mut workflow = Vec::new();
    let mut constraints = Vec::new();
    let mut best_practices = Vec::new();
    let mut version = None;
    let mut category = None;
    let mut required_tools = Vec::new();
    let mut delegation = Vec::new();
    let mut status = None;
    let mut schema_version = None;

    let (body_text, parsed_fm) = if let Some((yaml, body)) = split_frontmatter_local(markdown) {
        if let Ok(fm) = serde_yaml::from_str::<SkillFrontmatter>(&yaml) {
            name = fm.name;
            version = fm.version;
            category = fm.category;
            triggers = fm.triggers;
            required_tools = fm.required_tools;
            delegation = fm.delegation;
            status = fm.status;
            schema_version = fm.schema_version;
            (body, true)
        } else {
            (markdown, false)
        }
    } else {
        (markdown, false)
    };

    let mut current_section = "";

    for line in body_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("```") {
            continue;
        }
        if !parsed_fm {
            if let Some(n) = trimmed.strip_prefix("# Skill:") {
                name = n.trim().to_string();
                continue;
            }
        }
        match trimmed {
            t if t.starts_with("Description:") => {
                description = t.strip_prefix("Description:").unwrap().trim().to_string();
            }
            "Triggers:" => current_section = "triggers",
            "Workflow:" => current_section = "workflow",
            "Constraints:" => current_section = "constraints",
            "Best Practices:" => current_section = "best_practices",
            "## " | "# " => current_section = "",
            t if !t.is_empty() && !t.ends_with(':') => {
                let content = t
                    .trim_start_matches("- ")
                    .trim_start_matches("* ")
                    .to_string();
                match current_section {
                    "triggers" => {
                        if !parsed_fm {
                            triggers.push(content);
                        }
                    }
                    "workflow" => workflow.push(content),
                    "constraints" => constraints.push(content),
                    "best_practices" => best_practices.push(content),
                    _ => {}
                }
            }
            _ => {
                current_section = "";
            }
        }
    }

    if name.is_empty() {
        return None;
    }
    Some(SkillDefinition {
        name,
        description,
        triggers,
        workflow,
        constraints,
        best_practices,
        raw_markdown: markdown.to_string(),
        version,
        category,
        required_tools,
        delegation,
        status,
        schema_version,
    })
}

pub fn select_relevant_skills(query: &str, skills_dir: &Path) -> MoatResult<Vec<SkillDefinition>> {
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }
    let mut relevant: Vec<(i32, SkillDefinition)> = Vec::new();
    let query_lower = query.to_lowercase();
    for entry in std::fs::read_dir(skills_dir)
        .map_err(|e| MoatError::Settings(format!("failed to read skills dir: {e}")))?
    {
        let entry =
            entry.map_err(|e| MoatError::Settings(format!("failed to read skill entry: {e}")))?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let markdown = std::fs::read_to_string(&path)
            .map_err(|e| MoatError::Settings(format!("failed to read skill file: {e}")))?;
        if let Some(skill) = parse_skill_file(&markdown) {
            // Check status: only approved skills (or missing/default approved) are usable by agents
            let is_approved = skill.status.as_deref() != Some("draft");
            if is_approved {
                let mut score = 0;
                // 1. Trigger matches (+10)
                if skill
                    .triggers
                    .iter()
                    .any(|t| query_lower.contains(&t.to_lowercase()))
                {
                    score += 10;
                }
                // 2. Name matches (+5)
                if query_lower.contains(&skill.name.to_lowercase()) {
                    score += 5;
                }
                // 3. Category matches (+2)
                if let Some(ref cat) = skill.category {
                    if query_lower.contains(&cat.to_lowercase()) {
                        score += 2;
                    }
                }

                if score >= 2 {
                    relevant.push((score, skill));
                }
            }
        }
    }
    // Sort matching skills by score desc so more relevant skills come first
    relevant.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(relevant.into_iter().map(|(_, skill)| skill).collect())
}

pub fn select_instruction_skills(query: &str) -> MoatResult<Vec<SkillDefinition>> {
    let global_dir = config::skills_root();
    let swarm_skills_dir = config::swarm_root().join("skills");
    let mut dirs = vec![global_dir, swarm_skills_dir];

    if let Ok(curr_dir) = std::env::current_dir() {
        let local_dir = curr_dir.join("skills");
        if !dirs.contains(&local_dir) {
            dirs.push(local_dir);
        }
        if let Some(parent) = curr_dir.parent() {
            let parent_dir = parent.join("skills");
            if !dirs.contains(&parent_dir) {
                dirs.push(parent_dir);
            }
        }
    }

    // Discovery logging
    eprintln!("[Skills] Search paths:");
    for dir in &dirs {
        let exists = dir.exists();
        eprintln!("  {} {}", if exists { "✓" } else { "✗" }, dir.display());
    }

    let mut all_skills = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut discovered_files: Vec<String> = Vec::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)
            .map_err(|e| MoatError::Settings(format!("failed to read skills dir: {e}")))?
        {
            let entry = entry
                .map_err(|e| MoatError::Settings(format!("failed to read skill entry: {e}")))?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "md") {
                continue;
            }
            discovered_files.push(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
            let markdown = std::fs::read_to_string(&path)
                .map_err(|e| MoatError::Settings(format!("failed to read skill file: {e}")))?;
            if let Some(skill) = parse_skill_file(&markdown) {
                let is_approved = skill.status.as_deref() != Some("draft");
                if is_approved && seen.insert(skill.name.clone()) {
                    let mut score = 0;
                    let mut has_trigger_match = false;
                    let query_lower = query.to_lowercase();
                    // 1. Trigger matches (+10)
                    if skill
                        .triggers
                        .iter()
                        .any(|t| query_lower.contains(&t.to_lowercase()))
                    {
                        score += 10;
                        has_trigger_match = true;
                    }
                    // 2. Name matches (+5)
                    if query_lower.contains(&skill.name.to_lowercase()) {
                        score += 5;
                    }
                    // 3. Category matches (+2)
                    if let Some(ref cat) = skill.category {
                        if query_lower.contains(&cat.to_lowercase()) {
                            score += 2;
                        }
                    }

                    if has_trigger_match || score >= 5 {
                        eprintln!("[Skills] Selected: {} (score={})", skill.name, score);
                        all_skills.push((score, skill));
                    }
                }
            }
        }
    }

    if !discovered_files.is_empty() {
        eprintln!("[Skills] Discovered files: {}", discovered_files.join(", "));
    }
    if all_skills.is_empty() {
        eprintln!("[Skills] No skills matched query: {:?}", query);
    }

    // Sort by score desc
    all_skills.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(all_skills.into_iter().map(|(_, skill)| skill).collect())
}

pub fn save_skill_draft(name: &str, content: &str) -> MoatResult<PathBuf> {
    let dir = config::skills_root().join("generated_skills");
    std::fs::create_dir_all(&dir)
        .map_err(|e| MoatError::Settings(format!("failed to create generated_skills dir: {e}")))?;
    let path = dir.join(format!("draft_{name}.md"));
    std::fs::write(&path, content)
        .map_err(|e| MoatError::Settings(format!("failed to write skill draft: {e}")))?;
    Ok(path)
}

pub fn build_skills_manifest(skills_dir: &Path) -> MoatResult<SkillsManifest> {
    let mut skills = Vec::new();
    if !skills_dir.exists() {
        return Ok(SkillsManifest {
            schema_version: 1,
            generated_at: chrono::Utc::now().timestamp(),
            skills,
        });
    }
    for entry in std::fs::read_dir(skills_dir)
        .map_err(|e| MoatError::Settings(format!("failed to read skills dir: {e}")))?
    {
        let entry = entry.map_err(|e| MoatError::Settings(format!("failed to read entry: {e}")))?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(skill) = parse_skill_file(&content) {
                skills.push(SkillManifestEntry {
                    name: skill.name,
                    version: 1,
                    category: "general".into(),
                    priority: "medium".into(),
                    tags: skill.triggers.clone(),
                    triggers: skill.triggers,
                    file: path.file_name().unwrap().to_string_lossy().into(),
                    content_hash: sha256_hex(content.as_bytes()),
                });
            }
        }
    }
    Ok(SkillsManifest {
        schema_version: 1,
        generated_at: chrono::Utc::now().timestamp(),
        skills,
    })
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillsManifest {
    pub schema_version: u32,
    pub generated_at: i64,
    pub skills: Vec<SkillManifestEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillManifestEntry {
    pub name: String,
    pub version: u32,
    pub category: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub triggers: Vec<String>,
    pub file: String,
    pub content_hash: String,
}

pub const NATIVE_TOOL_NAMES: &[&str] = &[
    // Business core
    "identity:resolve",
    "opportunity:classify",
    "engagement:propose",
    "engagement:request_authorization",
    "engagement:authorize",
    "operator:escalate",
    "knowledge:query",
    // Infrastructure
    "fs:read",
    "fs:write",
    "process:spawn",
    "net:curl",
    "memory:search",
    "memory:learn",
    "meta:query_brain",
    "session:search",
    "propose_system_skill",
    "done",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SynthesizedSkill {
    pub skill_name: String,
    pub description: String,
    pub script_path: String,
    pub code_hash: String,
    pub language: String,
    pub parameter_schema: String,
    pub created_at: String,
}

impl SynthesizedSkill {
    pub fn parsed_parameter_schema(&self) -> MoatResult<Value> {
        serde_json::from_str(&self.parameter_schema).map_err(MoatError::Serde)
    }
}

#[derive(Debug, Clone)]
pub struct SkillProposal {
    pub skill_name: String,
    pub description: String,
    pub source_code: String,
    pub language: String,
    pub parameter_schema: String,
}

pub fn prepare_skills_table(conn: &Connection) -> MoatResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS synthesized_skills (
            skill_name TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            script_path TEXT NOT NULL,
            code_hash TEXT NOT NULL,
            language TEXT NOT NULL,
            parameter_schema TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        [],
    )
    ?;

    let mut has_parameter_schema = false;
    let mut stmt = conn
        .prepare("PRAGMA table_info(synthesized_skills)")
        ?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        ?;
    for row in rows {
        if row? == "parameter_schema" {
            has_parameter_schema = true;
            break;
        }
    }

    if !has_parameter_schema {
        conn.execute(
            "ALTER TABLE synthesized_skills ADD COLUMN parameter_schema TEXT NOT NULL DEFAULT '{}'",
            [],
        )
        ?;
    }

    Ok(())
}

pub fn ensure_skills_vault() -> MoatResult<PathBuf> {
    let root = config::skills_root();
    fs::create_dir_all(&root)
        .map_err(|err| MoatError::Settings(format!("failed to create skills vault: {err}")))?;
    Ok(root)
}

pub fn list_skills(conn: &Connection) -> MoatResult<Vec<SynthesizedSkill>> {
    let mut stmt = match conn.prepare(
        "SELECT skill_name, description, script_path, code_hash, language, parameter_schema, created_at
         FROM synthesized_skills
         ORDER BY created_at ASC, skill_name ASC",
    ) {
        Ok(stmt) => stmt,
        Err(err) if is_missing_skills_table(&err) => return Ok(Vec::new()),
        Err(err) => return Err(MoatError::Database(err)),
    };
    let rows = stmt
        .query_map([], |row| {
            Ok(SynthesizedSkill {
                skill_name: row.get(0)?,
                description: row.get(1)?,
                script_path: row.get(2)?,
                code_hash: row.get(3)?,
                language: row.get(4)?,
                parameter_schema: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        ?;

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row?);
    }
    Ok(skills)
}

pub fn select_relevant_synthesized_skills(
    conn: &Connection,
    query: &str,
) -> MoatResult<Vec<SynthesizedSkill>> {
    let all_skills = list_skills(conn)?;
    if query.trim().is_empty() {
        return Ok(all_skills);
    }

    let query_words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();

    if query_words.is_empty() {
        return Ok(all_skills);
    }

    let mut ranked: Vec<(usize, SynthesizedSkill)> = all_skills
        .into_iter()
        .map(|skill| {
            let name_lower = skill.skill_name.to_lowercase();
            let desc_lower = skill.description.to_lowercase();
            let mut score = 0;
            for word in &query_words {
                if name_lower.contains(word) {
                    score += 10;
                }
                if desc_lower.contains(word) {
                    score += 2;
                }
            }
            (score, skill)
        })
        .filter(|(score, _)| *score > 0)
        .collect();

    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(ranked.into_iter().map(|(_, skill)| skill).collect())
}

pub fn get_skill(conn: &Connection, skill_name: &str) -> MoatResult<Option<SynthesizedSkill>> {
    match conn.query_row(
        "SELECT skill_name, description, script_path, code_hash, language, parameter_schema, created_at
         FROM synthesized_skills
         WHERE skill_name = ?1",
        params![skill_name.trim()],
        |row| {
            Ok(SynthesizedSkill {
                skill_name: row.get(0)?,
                description: row.get(1)?,
                script_path: row.get(2)?,
                code_hash: row.get(3)?,
                language: row.get(4)?,
                parameter_schema: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    ) {
        Ok(skill) => Ok(Some(skill)),
        Err(err) if is_missing_skills_table(&err) => Ok(None),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(MoatError::Database(err)),
    }
}

pub fn install_skill(conn: &Connection, proposal: &SkillProposal) -> MoatResult<SynthesizedSkill> {
    let skill_name = normalize_skill_name(&proposal.skill_name)?;
    if native_tool_name_collision(&skill_name) {
        return Err(MoatError::InvariantViolation(format!(
            "skill name '{}' collides with a built-in tool",
            skill_name
        )));
    }
    if get_skill(conn, &skill_name)?.is_some() {
        return Err(MoatError::InvariantViolation(format!(
            "skill '{}' is already installed",
            skill_name
        )));
    }

    let language = normalize_language(&proposal.language)?;
    let parameter_schema = normalize_parameter_schema(&proposal.parameter_schema)?;
    let description = proposal.description.trim();
    let source_code = proposal.source_code.as_bytes();
    if description.is_empty() {
        return Err(MoatError::Orchestrator(
            "skill description cannot be empty".into(),
        ));
    }
    if source_code.is_empty() {
        return Err(MoatError::Orchestrator(
            "skill source code cannot be empty".into(),
        ));
    }

    let vault_root = ensure_skills_vault()?;
    let extension = language_extension(&language);
    let script_path = vault_root.join(format!("{skill_name}.{extension}"));
    let tmp_path = vault_root.join(format!("{skill_name}.{extension}.tmp"));
    let code_hash = sha256_hex(source_code);
    let created_at = Utc::now().to_rfc3339();

    fs::write(&tmp_path, source_code)
        .map_err(|err| MoatError::Settings(format!("failed to write temp skill file: {err}")))?;
    if let Err(err) = fs::rename(&tmp_path, &script_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(MoatError::Settings(format!(
            "failed to finalize skill install: {err}"
        )));
    }

    if let Err(err) = set_vault_readonly(&script_path) {
        return Err(MoatError::Settings(format!(
            "failed to lock installed skill file: {err}"
        )));
    }

    let record = SynthesizedSkill {
        skill_name,
        description: description.to_string(),
        script_path: script_path.to_string_lossy().into_owned(),
        code_hash,
        language,
        parameter_schema,
        created_at,
    };

    if let Err(err) = conn.execute(
        "INSERT INTO synthesized_skills (
            skill_name, description, script_path, code_hash, language, parameter_schema, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.skill_name,
            record.description,
            record.script_path,
            record.code_hash,
            record.language,
            record.parameter_schema,
            record.created_at,
        ],
    ) {
        let _ = fs::remove_file(&record.script_path);
        return Err(MoatError::Database(err));
    }

    Ok(record)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn native_tool_name_collision(name: &str) -> bool {
    NATIVE_TOOL_NAMES
        .iter()
        .any(|native| native.eq_ignore_ascii_case(name.trim()))
}

pub fn normalize_skill_name(raw: &str) -> MoatResult<String> {
    let candidate = raw.trim();
    if candidate.is_empty() {
        return Err(MoatError::Orchestrator("skill_name cannot be empty".into()));
    }
    if !candidate
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        return Err(MoatError::Orchestrator(
            "skill_name must use lowercase letters, digits, '_' or '-'".into(),
        ));
    }
    Ok(candidate.to_string())
}

pub fn normalize_language(raw: &str) -> MoatResult<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "python" => Ok("python".to_string()),
        "bash" => Ok("bash".to_string()),
        other => Err(MoatError::Orchestrator(format!(
            "unsupported skill language '{}'; expected 'python' or 'bash'",
            other
        ))),
    }
}

pub fn language_extension(language: &str) -> &'static str {
    match language {
        "python" => "py",
        "bash" => "sh",
        _ => "txt",
    }
}

pub fn interpreter_for_language(language: &str) -> &'static str {
    match language {
        "python" => "python3",
        "bash" => "bash",
        _ => "bash",
    }
}

pub fn normalize_parameter_schema(raw: &str) -> MoatResult<String> {
    let value: Value = serde_json::from_str(raw).map_err(MoatError::Serde)?;
    if !value.is_object() {
        return Err(MoatError::Orchestrator(
            "skill parameters must be a JSON object schema".into(),
        ));
    }
    serde_json::to_string(&value).map_err(MoatError::Serde)
}

#[cfg(unix)]
fn set_vault_readonly(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o555);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_vault_readonly(path: &Path) -> std::io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms)
}

fn is_missing_skills_table(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(message)) => {
            message.contains("no such table: synthesized_skills")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_skill_name_rejects_invalid_characters() {
        assert!(normalize_skill_name("valid_skill-1").is_ok());
        assert!(normalize_skill_name("Invalid Skill").is_err());
        assert!(normalize_skill_name("../escape").is_err());
    }

    #[test]
    fn normalize_parameter_schema_requires_object() {
        assert!(normalize_parameter_schema("{\"url\":\"string\"}").is_ok());
        assert!(normalize_parameter_schema("[\"bad\"]").is_err());
    }

    #[test]
    fn parse_skill_file_with_frontmatter() {
        let markdown = r#"---
name: test_skill
version: 2
category: test
triggers:
  - trigger one
  - trigger two
required_tools:
  - tool:one
delegation:
  - agent:one
status: approved
---
Description: A test skill with frontmatter.

Workflow:
* Step one
* Step two
"#;
        let skill = parse_skill_file(markdown).expect("parse skill");
        assert_eq!(skill.name, "test_skill");
        assert_eq!(skill.version, Some(2));
        assert_eq!(skill.category.as_deref(), Some("test"));
        assert_eq!(skill.triggers, vec!["trigger one", "trigger two"]);
        assert_eq!(skill.required_tools, vec!["tool:one"]);
        assert_eq!(skill.delegation, vec!["agent:one"]);
        assert_eq!(skill.status.as_deref(), Some("approved"));
        assert_eq!(skill.description, "A test skill with frontmatter.");
        assert_eq!(skill.workflow, vec!["Step one", "Step two"]);
    }

    #[test]
    fn test_select_relevant_skills_filtering() {
        let temp_dir = std::env::temp_dir().join(format!("skills_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let s1 = r#"---
name: browser_ai
status: approved
category: browser
triggers:
  - ask chatgpt
schema_version: 1
---
Description: approved browser AI skill
"#;
        let s2 = r#"---
name: code_debug
status: draft
category: debugger
triggers:
  - debug crash
schema_version: 1
---
Description: draft debugging skill
"#;
        std::fs::write(temp_dir.join("browser_ai.md"), s1).unwrap();
        std::fs::write(temp_dir.join("code_debug.md"), s2).unwrap();

        // 1. Matches approved skill by name (ranks with score +5)
        let matched = select_relevant_skills("browser_ai", &temp_dir).unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "browser_ai");
        assert_eq!(matched[0].schema_version, Some(1));

        // 2. Matches approved skill by trigger (ranks with score +10)
        let matched = select_relevant_skills("ask chatgpt to write rust", &temp_dir).unwrap();
        assert_eq!(matched.len(), 1);

        // 3. Matches approved skill by category (ranks with score +2)
        let matched = select_relevant_skills("do some browser tasks", &temp_dir).unwrap();
        assert_eq!(matched.len(), 1);

        // 4. Draft skill is filtered out/ignored
        let matched = select_relevant_skills("debug crash", &temp_dir).unwrap();
        assert_eq!(matched.len(), 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_select_relevant_skills_ranking() {
        let temp_dir = std::env::temp_dir().join(format!("skills_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let s1 = r#"---
name: browser_ai
status: approved
category: browser
triggers:
  - ask chatgpt
---
Description: approved browser AI skill
"#;
        let s2 = r#"---
name: browser_history
status: approved
category: browser
triggers:
  - view history
---
Description: browser history viewer skill
"#;
        std::fs::write(temp_dir.join("browser_ai.md"), s1).unwrap();
        std::fs::write(temp_dir.join("browser_history.md"), s2).unwrap();

        // Querying "ask chatgpt browser":
        // s1: Matches trigger (+10) + category (+2) = 12
        // s2: Matches category (+2) = 2
        // Sorted: s1 first, then s2
        let matched = select_relevant_skills("ask chatgpt browser", &temp_dir).unwrap();
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].name, "browser_ai");
        assert_eq!(matched[1].name, "browser_history");

        // Querying "view history browser":
        // s1: Matches category (+2) = 2
        // s2: Matches trigger (+10) + category (+2) = 12
        // Sorted: s2 first, then s1
        let matched = select_relevant_skills("view history browser", &temp_dir).unwrap();
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].name, "browser_history");
        assert_eq!(matched[1].name, "browser_ai");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_select_instruction_skills_scoring_thresholds() {
        let s1 = r#"---
name: browser_ai_interaction
status: approved
category: browser
triggers:
  - ask chatgpt
  - prompt chatgpt
  - send to claude
  - ask gemini
  - chatgpt
  - chatcpt
  - gemini
  - claude
  - perplexity
---
Description: approved browser AI skill
"#;

        let parse_and_score = |query: &str, content: &str| -> Option<SkillDefinition> {
            let skill = parse_skill_file(content)?;
            let query_lower = query.to_lowercase();
            let mut score = 0;
            let mut has_trigger_match = false;
            if skill
                .triggers
                .iter()
                .any(|t| query_lower.contains(&t.to_lowercase()))
            {
                score += 10;
                has_trigger_match = true;
            }
            if query_lower.contains(&skill.name.to_lowercase()) {
                score += 5;
            }
            if let Some(ref cat) = skill.category {
                if query_lower.contains(&cat.to_lowercase()) {
                    score += 2;
                }
            }
            if has_trigger_match || score >= 5 {
                Some(skill)
            } else {
                None
            }
        };

        // Scenario 1: Objective: "Open ChatGPT and ask for my name"
        let matched = parse_and_score("Open ChatGPT and ask for my name", s1);
        assert!(
            matched.is_some(),
            "Should match browser_ai_interaction because of trigger match"
        );

        // Scenario 2: Objective: "Fix a Rust borrow checker issue"
        let matched = parse_and_score("Fix a Rust borrow checker issue", s1);
        assert!(matched.is_none(), "Should NOT match browser_ai_interaction");

        // Scenario 3: Objective: "browser cookies"
        let matched = parse_and_score("browser cookies", s1);
        assert!(
            matched.is_none(),
            "Should NOT match browser_ai_interaction (category-only match excluded)"
        );
    }
}
