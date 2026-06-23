#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub rule_name: String,
    pub description: String,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationSeverity {
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub violations: Vec<PolicyViolation>,
}

pub struct PolicyValidator;

impl Default for PolicyValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(
        &self,
        response: &str,
        business_rules: &[Box<dyn PolicyRule>],
    ) -> ValidationResult {
        let mut violations = Vec::new();

        for rule in business_rules.iter() {
            if let Some(violation) = rule.evaluate(response) {
                violations.push(violation);
            }
        }

        let has_critical = violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Critical);

        ValidationResult {
            passed: !has_critical,
            violations,
        }
    }
}

pub trait PolicyRule: Send + Sync {
    fn evaluate(&self, text: &str) -> Option<PolicyViolation>;
    fn rule_name(&self) -> &str;
}

pub struct NoUnauthorizedDiscounts;
impl PolicyRule for NoUnauthorizedDiscounts {
    fn evaluate(&self, text: &str) -> Option<PolicyViolation> {
        let discount_patterns = [
            ("free of charge", "unauthorized free service promise"),
            ("at no cost", "unauthorized zero-cost commitment"),
            ("no payment required", "unauthorized payment waiver"),
        ];
        for (pattern, desc) in &discount_patterns {
            if text.to_lowercase().contains(pattern) {
                return Some(PolicyViolation {
                    rule_name: self.rule_name().to_string(),
                    description: format!("Potential unauthorized commitment: {desc}"),
                    severity: ViolationSeverity::Critical,
                });
            }
        }
        None
    }

    fn rule_name(&self) -> &str {
        "no_unauthorized_discounts"
    }
}

pub struct NoUnauthorizedRefunds;
impl PolicyRule for NoUnauthorizedRefunds {
    fn evaluate(&self, text: &str) -> Option<PolicyViolation> {
        if text.to_lowercase().contains("full refund")
            || text.to_lowercase().contains("money back guarantee")
        {
            return Some(PolicyViolation {
                rule_name: self.rule_name().to_string(),
                description: "Potential unauthorized refund or money-back guarantee promise"
                    .to_string(),
                severity: ViolationSeverity::Critical,
            });
        }
        None
    }

    fn rule_name(&self) -> &str {
        "no_unauthorized_refunds"
    }
}

pub struct NoFalseCommitments;
impl PolicyRule for NoFalseCommitments {
    fn evaluate(&self, text: &str) -> Option<PolicyViolation> {
        let lower = text.to_lowercase();
        if lower.contains("i guarantee")
            || lower.contains("i promise")
            || lower.contains("i assure you")
        {
            return Some(PolicyViolation {
                rule_name: self.rule_name().to_string(),
                description: "Potential false commitment or guarantee made by AI".to_string(),
                severity: ViolationSeverity::Warning,
            });
        }
        None
    }

    fn rule_name(&self) -> &str {
        "no_false_commitments"
    }
}

pub fn default_rules() -> Vec<Box<dyn PolicyRule>> {
    vec![
        Box::new(NoUnauthorizedDiscounts),
        Box::new(NoUnauthorizedRefunds),
        Box::new(NoFalseCommitments),
    ]
}
