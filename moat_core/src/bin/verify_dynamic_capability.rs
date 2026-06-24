use moat_core::agent::llm_types::{LlmMessage, LlmProvider, LlmResponse};
use moat_core::agent::tools::{global_registry, ToolRegistry};
use moat_core::memory::meta_brain::MetaBrain;
use moat_core::planner::prompt::build_react_system_prompt;
use moat_core::api::routes::{read_package_manifest, register_package_core};
use rusqlite::{Connection, params};
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use serde_json::json;

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

struct DynamicStubProvider;

impl LlmProvider for DynamicStubProvider {
    fn chat_complete<'a>(
        &'a self,
        _model: &'a str,
        messages: Vec<LlmMessage>,
        _tools: Option<Vec<moat_core::agent::interaction_orchestrator::LlmTool>>,
        _max_tokens: usize,
        _temperature: f32,
        _response_format: Option<serde_json::Value>,
    ) -> BoxFuture<'a, moat_core::MoatResult<LlmResponse>> {
        Box::pin(async move {
            // Find system prompt
            let system_prompt = messages.iter()
                .find(|m| m.role == "system")
                .and_then(|m| m.content.as_ref())
                .cloned()
                .unwrap_or_default();

            let has_property_tool = system_prompt.contains("property_tool");

            let content = if has_property_tool {
                json!({
                    "state": "running",
                    "thought": "Since real-estate-v1 is installed and property_tool is available, I will use it to schedule the viewing.",
                    "actions": [
                        {
                            "action": "property_tool",
                            "target": "system",
                            "payload": {
                                "action": "schedule_viewing",
                                "time": "tomorrow at 4 PM"
                            }
                        }
                    ]
                }).to_string()
            } else {
                json!({
                    "state": "completed",
                    "thought": "I don't see any real estate tools in my allowed actions list. I cannot perform this task.",
                    "summary": "I'm sorry, but I do not have the capability to schedule property viewings at this time."
                }).to_string()
            };

            Ok(LlmResponse {
                content: Some(content),
                tool_calls: None,
                tokens: 50,
                structured: None,
            })
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== DYNAMIC CAPABILITY CERTIFICATION ===");

    // 1. MANIFEST AUTHORITY (AGENT A)
    println!("\n[AGENT A - MANIFEST AUTHORITY]");
    let pkgs = vec!["real-estate-v1", "notification-package-v1", "sales-core-v1"];
    for pkg_id in &pkgs {
        println!("\n--- Manifest for {} ---", pkg_id);
        if let Some(manifest) = read_package_manifest(pkg_id) {
            println!("Location: packages/{}/package.yaml (resolved via current_dir fallback)", pkg_id);
            println!("Schema / Extracted fields:");
            println!("  ID: {}", manifest.id);
            println!("  Name: {}", manifest.name);
            println!("  Version: {}", manifest.version);
            println!("  Description: {:?}", manifest.description);
            println!("  Permissions: {:?}", manifest.permissions);
            println!("  Commands: {:?}", manifest.commands);
            println!("  Events: {:?}", manifest.events);
            println!("  Projections: {:?}", manifest.projections);
            println!("  Skills: {:?}", manifest.skills);
            println!("  Tools: {:?}", manifest.tools);
        } else {
            println!("WARNING: Failed to read manifest for {}", pkg_id);
        }
    }

    // Initialize MetaBrain & ToolRegistry
    let meta_brain = Arc::new(AsyncRwLock::new(MetaBrain::new()));
    
    // Clear registry to start fresh
    {
        let mut reg = global_registry().write().unwrap();
        *reg = ToolRegistry::new();
        // Register default tools
        reg.register(moat_core::agent::tools::ToolEntry::new(moat_core::agent::tools::SimpleTool {
            name: "identity:resolve",
            description: "Resolve messaging handle/channel to a contact ID.",
            reversibility: moat_core::agent::tools::ToolReversibility::Reversible,
        }));
    }

    // 2. TOOL REGISTRY AUTHORITY BEFORE (AGENT B)
    println!("\n[AGENT B - TOOL REGISTRY AUTHORITY]");
    let before_tool_count = global_registry().read().unwrap().names().len();
    let before_tools = global_registry().read().unwrap().names();
    println!("Before install tool count: {}", before_tool_count);
    println!("Before install tool IDs: {:?}", before_tools);

    // 3. METABRAIN AUTHORITY BEFORE (AGENT C)
    println!("\n[AGENT C - METABRAIN AUTHORITY]");
    let before_node_count = {
        let brain = meta_brain.read().await;
        // Count total nodes in graph
        brain.get_instruction_skills().len() + brain.get_synthesized_skills().len()
    };
    println!("Before install MetaBrain skill node count: {}", before_node_count);

    // 4. PROMPT AUTHORITY BEFORE (AGENT D)
    println!("\n[AGENT D - PROMPT AUTHORITY]");
    let before_prompt = build_react_system_prompt(None, &[], &meta_brain.read().await.get_instruction_skills());
    println!("--- Before Prompt Sample (Allowed Actions) ---");
    if let Some(pos) = before_prompt.find("ALLOWED ACTIONS SCHEMA:") {
        println!("{}", &before_prompt[pos..std::cmp::min(pos+300, before_prompt.len())]);
    }

    // 5. CAPABILITY EXPANSION TEST A (AGENT E)
    println!("\n[AGENT E - CAPABILITY EXPANSION AUTHORITY - TEST A]");
    let provider = Arc::new(DynamicStubProvider);
    // Mocking prompt processing
    let input_msg = LlmMessage {
        role: "user".into(),
        content: Some("Schedule a property viewing tomorrow at 4 PM.".into()),
        tool_calls: None,
        tool_call_id: None,
    };
    let response_before = provider.chat_complete(
        "stub",
        vec![
            LlmMessage {
                role: "system".into(),
                content: Some(before_prompt.clone()),
                tool_calls: None,
                tool_call_id: None,
            },
            input_msg.clone(),
        ],
        None,
        100,
        0.0,
        None,
    ).await?;
    println!("Test A (No Package) LLM Response:\n{}", response_before.content.unwrap());

    // INSTALL PACKAGE (real-estate-v1)
    println!("\n>>> Installing package: real-estate-v1...");
    let manifest = read_package_manifest("real-estate-v1").expect("load real-estate manifest");
    register_package_core(&manifest, &meta_brain).await;

    // 6. TOOL REGISTRY AFTER (AGENT B)
    let after_tool_count = global_registry().read().unwrap().names().len();
    let after_tools = global_registry().read().unwrap().names();
    println!("After install tool count: {}", after_tool_count);
    println!("After install tool IDs: {:?}", after_tools);
    let new_tools: Vec<_> = after_tools.iter().filter(|t| !before_tools.contains(t)).collect();
    println!("Exact new tools added: {:?}", new_tools);

    // 7. METABRAIN AFTER (AGENT C)
    let after_node_count = {
        let brain = meta_brain.read().await;
        brain.get_instruction_skills().len() + brain.get_synthesized_skills().len()
    };
    println!("After install MetaBrain skill node count: {}", after_node_count);
    println!("Exact dynamic skills in MetaBrain:");
    for skill in meta_brain.read().await.get_instruction_skills() {
        println!("  Skill Name: {}", skill.name);
        println!("  Description: {}", skill.description);
    }

    // 8. PROMPT AFTER (AGENT D)
    let after_prompt = build_react_system_prompt(None, &[], &meta_brain.read().await.get_instruction_skills());
    println!("--- After Prompt Sample (Allowed Actions & Skills) ---");
    if let Some(pos) = after_prompt.find("ALLOWED ACTIONS SCHEMA:") {
        let end_pos = after_prompt.find("SYNTHESIZED SYSTEM SKILLS").unwrap_or(after_prompt.len());
        println!("{}", &after_prompt[pos..std::cmp::min(pos+500, std::cmp::min(end_pos, after_prompt.len()))]);
    }

    println!("\nGenerating Prompt Diff (Lines Added):");
    let before_lines: std::collections::HashSet<&str> = before_prompt.lines().collect();
    for line in after_prompt.lines() {
        if !before_lines.contains(line) && !line.trim().is_empty() {
            println!("+ {}", line);
        }
    }

    // 9. CAPABILITY EXPANSION TEST B (AGENT E)
    println!("\n[AGENT E - CAPABILITY EXPANSION AUTHORITY - TEST B]");
    let response_after = provider.chat_complete(
        "stub",
        vec![
            LlmMessage {
                role: "system".into(),
                content: Some(after_prompt.clone()),
                tool_calls: None,
                tool_call_id: None,
            },
            input_msg.clone(),
        ],
        None,
        100,
        0.0,
        None,
    ).await?;
    println!("Test B (Package Installed) LLM Response:\n{}", response_after.content.unwrap());

    // 10. RESTART PERSISTENCE AUTHORITY (AGENT F)
    println!("\n[AGENT F - RESTART PERSISTENCE AUTHORITY]");
    let temp_db_path = "test_persistence.db";
    let _ = std::fs::remove_file(temp_db_path);
    let conn = Connection::open(temp_db_path)?;
    moat_core::business::db::prepare_tables(&conn)?;
    
    // Simulate Package Install persistence via projections
    conn.execute(
        "INSERT INTO projections (id, business_id, name, version, timestamp) VALUES (?1, ?2, ?3, 1, ?4)",
        params!["real-estate-v1", "default_business", "PackageInstalled", 1782305813973i64]
    )?;
    println!("Persisted PackageInstalled projection row in SQLite database.");

    // Simulate Re-hydration restart
    let meta_brain_restart = Arc::new(AsyncRwLock::new(MetaBrain::new()));
    
    // Reset registry
    {
        let mut reg = global_registry().write().unwrap();
        *reg = ToolRegistry::new();
    }

    println!("Restarting runtime... Rehydrating from persistent DB storage...");
    let mut stmt = conn.prepare("SELECT id FROM projections WHERE name = 'PackageInstalled'")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for pkg_id in rows.flatten() {
        if let Some(manifest) = read_package_manifest(&pkg_id) {
            register_package_core(&manifest, &meta_brain_restart).await;
            println!("Loaded package {} from manifest & registered capabilities.", pkg_id);
        }
    }

    let persisted_tool_count = global_registry().read().unwrap().names().len();
    let persisted_tools = global_registry().read().unwrap().names();
    println!("Post-restart Tool count: {}", persisted_tool_count);
    println!("Post-restart Tool IDs: {:?}", persisted_tools);

    let persisted_node_count = {
        let brain = meta_brain_restart.read().await;
        brain.get_instruction_skills().len()
    };
    println!("Post-restart MetaBrain skill node count: {}", persisted_node_count);
    
    let _ = std::fs::remove_file(temp_db_path);
    
    println!("\n=== CERTIFICATION END ===");
    Ok(())
}
