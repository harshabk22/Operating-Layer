# E2E Test Infra: MoatCore Type D Package Refactoring

## Test Philosophy
- Opaque-box, requirement-driven. No dependency on implementation design.
- Methodology: Category-Partition + BVA + Pairwise + Workload Testing.

## Feature Inventory
| # | Feature | Source (requirement) | Tier 1 | Tier 2 | Tier 3 |
|---|---------|---------------------|:------:|:------:|:------:|
| 1 | Dynamic Package Engine Integration | ORIGINAL_REQUEST §1 / PROJECT.md M1 | 5 | 5 | ✓ |
| 2 | Packages Install/Uninstall | ORIGINAL_REQUEST §4 | 5 | 5 | ✓ |
| 3 | Allowed Actions Gating | ORIGINAL_REQUEST §4 | 5 | 5 | ✓ |
| 4 | MetaBrain / Consent Bridge | ORIGINAL_REQUEST §4 | 5 | 5 | ✓ |

## Test Architecture
- Test runner: Playwright (E2E), Cargo test (unit/integration)
- Test case format: Playwright test files (.spec.ts)
- Directory layout:
  - Playwright tests: `/e2e/tests/`
  - Rust unit tests: `/moat_core/src/`

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | Install/Uninstall/Re-install sequence | Packages Install/Uninstall, Allowed Actions, MetaBrain | High |
| 2 | Policy enforcement on unauthorized tool invocation | Allowed Actions, MetaBrain | High |

## Coverage Thresholds
- Tier 1: ≥5 per feature
- Tier 2: ≥5 per feature (where boundaries exist)
- Tier 3: pairwise coverage of major feature interactions
- Tier 4: ≥5 realistic application scenarios
