# Original User Request

## Initial Request — 2026-06-24T10:45:26+05:30

# Teamwork Project Prompt — Draft

> Status: Ready for launch — awaiting user approval
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Refactor the MoatAgeex package architecture from Type A (UI/Backend feature modules) to Type D (Dynamic Agent Extensions) where package installation registers skills, mutates agent prompts, updates MetaBrain, and physically expands capabilities, while aggressively eradicating legacy architecture and dead code.

Working directory: /Users/harshavardhanbk/Desktop/MoatAgeex
Integrity mode: benchmark

## Requirements

### R1. Dynamic Package Engine (Swarms A-E)
Create a single authoritative package manifest format. Connect package installation and uninstallation to the skill framework, tool registry, MetaBrain, and prompt generation (`build_react_system_prompt`). Agent capabilities and behavior must physically expand and contract based solely on the package install state.

### R2. Legacy Eradication (Swarms F-I)
Delete all dead systems, V1 legacy architecture, unused routes, decorative pages, and orphaned database tables. Consolidate packages to keep only `sales-core-v1`, `real-estate-v1`, and `notification-package-v1`. Ensure a single authoritative database and UI architecture.

### R3. Production Certification (Swarm J)
The final state must not regress any production functionality. It must be proven via programmatic verification.

## Acceptance Criteria

### Architectural Capabilities
- [ ] DYNAMIC_SKILL_REGISTRATION = PASS (SkillDefinition objects created and stored upon package install)
- [ ] DYNAMIC_TOOL_REGISTRATION = PASS (Tools are injected into Planner automatically)
- [ ] PROMPT_MUTATION = PASS (System prompt demonstrably changes after package install)
- [ ] METABRAIN_INTEGRATION = PASS (Package skills register correctly in MetaBrain memory/graph)
- [ ] PACKAGE_CAPABILITY_EXPANSION = PASS (Agent successfully completes domain task ONLY when corresponding package is installed)

### Legacy Eradication
- [ ] LEGACY_RUNTIME_REMOVED = PASS (No V1 legacy components remain)
- [ ] ORPHAN_CODE_REMOVED = PASS (Dead schemas/modules removed)
- [ ] ORPHAN_PACKAGES_REMOVED = PASS (Only sales-core, real-estate, and notification packages remain)
- [ ] ORPHAN_ROUTES_REMOVED = PASS (UI routing maps cleanly to backend commands)

### Production Certification
- [ ] PRODUCTION_CERTIFICATION = PASS (`cargo check`, `cargo clippy -D warnings`, `cargo test`, `npm run build`, and Playwright suites pass cleanly)
- [ ] FINAL_PACKAGE_ARCHITECTURE = TYPE_D
