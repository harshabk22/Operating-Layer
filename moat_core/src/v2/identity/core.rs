use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tenant {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Permission {
    pub id: String,
    pub resource: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub permissions: HashSet<Permission>,
    pub parent_role_id: Option<String>,
}

pub struct RBACValidator {
    roles: HashMap<String, Role>,
}

impl Default for RBACValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl RBACValidator {
    pub fn new() -> Self {
        Self {
            roles: HashMap::new(),
        }
    }

    pub fn add_role(&mut self, role: Role) {
        self.roles.insert(role.id.clone(), role);
    }

    pub fn has_permission(&self, role_id: &str, resource: &str, action: &str) -> bool {
        let mut current_role_id = Some(role_id.to_string());
        let mut visited = HashSet::new();

        while let Some(id) = current_role_id {
            if visited.contains(&id) {
                break;
            }
            visited.insert(id.clone());

            if let Some(role) = self.roles.get(&id) {
                for perm in &role.permissions {
                    if perm.resource == resource && perm.action == action {
                        return true;
                    }
                }
                current_role_id = role.parent_role_id.clone();
            } else {
                break;
            }
        }

        false
    }
}

pub struct CapabilityGuard {
    pub tenant: Tenant,
    pub role_id: String,
    pub validator: Arc<RBACValidator>,
}

impl CapabilityGuard {
    pub fn new(tenant: Tenant, role_id: String, validator: Arc<RBACValidator>) -> Self {
        Self {
            tenant,
            role_id,
            validator,
        }
    }

    pub fn authorize(&self, resource: &str, action: &str) -> Result<(), String> {
        if self
            .validator
            .has_permission(&self.role_id, resource, action)
        {
            Ok(())
        } else {
            Err(format!(
                "Authorization failed for tenant {} (id: {}): role {} does not have permission {}:{}",
                self.tenant.name, self.tenant.id, self.role_id, action, resource
            ))
        }
    }
}
