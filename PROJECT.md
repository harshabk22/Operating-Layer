# Project: MoatAgeex Type D Refactoring

## Architecture
- Refactor the MoatAgeex package system from Type A (hardcoded endpoints/routes) to Type D (dynamic package engine).
- Implement dynamic skill registration, dynamic tool registration, system prompt mutation, and MetaBrain policy graph updates on package install/uninstall.
- Clean up legacy frontend page files (`Approvals.tsx`, `Calendar.tsx`), backend Axum routes/handlers (`/operations/approvals`, `/operations/visits`), SQLite approvals table, and incomplete supervisor task down migrations.

## Code Layout
- Package source: `/moat_core/src/v2/package/`
- API routes: `/moat_core/src/api/routes.rs`
- Server setup: `/moat_core/src/api/server.rs`
- System prompt generation: `/moat_core/src/planner/prompt.rs`
- MetaBrain graph: `/moat_core/src/memory/meta_brain.rs`
- Client side page files: `/frontend/src/pages/`
- Playwright E2E suite: `/e2e/tests/`

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Dynamic Package Engine Integration | Implement manifest validation, V2 Package registry connection, and database projection mapping | none | PLANNED |
| 2 | Capabilities Expansion & Prompt Mutation | Connect package install/uninstall to dynamic skills, MetaBrain graph, allowed_tools, and build_react_system_prompt | M1 | PLANNED |
| 3 | Legacy System Eradication | Delete unused views, endpoints, database tables, and fix supervisor task down migration | none | PLANNED |
| 4 | Production Certification & E2E Validation | Run all compiler checks, cargo test workspace, npm build, and Playwright tests | M1, M2, M3 | PLANNED |

## Interface Contracts
### Package Installer API
- POST `/platform/packages/install` -> Accepts `{ "package_id": String }` or `{ "package": String }` or `{ "manifest": Manifest }`. Resolves dependencies, updates SQLite projections, and registers skills/tools/MetaBrain nodes.
- DELETE `/platform/packages/:id` -> Uninstalls package, removes registered tools/skills/MetaBrain nodes, and updates SQLite projections.
