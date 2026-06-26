use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, RwLock};
use thiserror::Error;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RouteEntry {
    pub path: String,
    pub component: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NavItem {
    pub label: String,
    pub path: String,
    pub icon: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NavGroup {
    pub label: String,
    pub items: Vec<NavItem>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PackageManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
    #[serde(default)]
    pub navigation_entries: Vec<NavGroup>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub projections: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PackageVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PackageVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageDependency {
    pub name: String,
    pub version: PackageVersion,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: PackageVersion,
    pub dependencies: Vec<PackageDependency>,
    pub defines_runtime_primitives: bool,
}

#[derive(Error, Debug, PartialEq)]
pub enum PackageError {
    #[error("Package {0} redefines runtime primitives")]
    RedefinesRuntimePrimitives(String),
    #[error("Dependency cycle detected")]
    CyclicDependency,
    #[error("Dependency {0} v{1} not found")]
    DependencyNotFound(String, PackageVersion),
    #[error("Package {0} v{1} already exists")]
    PackageAlreadyExists(String, PackageVersion),
    #[error("Package {0} v{1} not found")]
    PackageNotFound(String, PackageVersion),
}

pub struct PackageRegistry {
    packages: Arc<RwLock<HashMap<String, HashMap<PackageVersion, Package>>>>,
}

impl PackageRegistry {
    pub fn new() -> Self {
        Self {
            packages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, package: Package) -> Result<(), PackageError> {
        let mut pkgs = self.packages.write().unwrap();
        let versions = pkgs
            .entry(package.name.clone())
            .or_default();
        if versions.contains_key(&package.version) {
            return Err(PackageError::PackageAlreadyExists(
                package.name,
                package.version,
            ));
        }
        versions.insert(package.version.clone(), package);
        Ok(())
    }

    pub fn get(&self, name: &str, version: &PackageVersion) -> Option<Package> {
        let pkgs = self.packages.read().unwrap();
        pkgs.get(name)
            .and_then(|versions| versions.get(version).cloned())
    }

    pub fn list_versions(&self, name: &str) -> Vec<PackageVersion> {
        let pkgs = self.packages.read().unwrap();
        if let Some(versions) = pkgs.get(name) {
            let mut v: Vec<_> = versions.keys().cloned().collect();
            v.sort();
            v
        } else {
            vec![]
        }
    }
}

impl Default for PackageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PackageValidator;

impl PackageValidator {
    pub fn validate(pkg: &Package, registry: &PackageRegistry) -> Result<(), PackageError> {
        if pkg.defines_runtime_primitives {
            return Err(PackageError::RedefinesRuntimePrimitives(pkg.name.clone()));
        }

        let mut visited = HashSet::new();
        let mut path = HashSet::new();
        Self::check_cycles(pkg, registry, &mut visited, &mut path)?;

        Ok(())
    }

    fn check_cycles(
        pkg: &Package,
        registry: &PackageRegistry,
        visited: &mut HashSet<(String, PackageVersion)>,
        path: &mut HashSet<(String, PackageVersion)>,
    ) -> Result<(), PackageError> {
        let node = (pkg.name.clone(), pkg.version.clone());
        if path.contains(&node) {
            return Err(PackageError::CyclicDependency);
        }
        if visited.contains(&node) {
            return Ok(());
        }

        visited.insert(node.clone());
        path.insert(node.clone());

        for dep in &pkg.dependencies {
            let dep_pkg = registry.get(&dep.name, &dep.version).ok_or_else(|| {
                PackageError::DependencyNotFound(dep.name.clone(), dep.version.clone())
            })?;
            Self::check_cycles(&dep_pkg, registry, visited, path)?;
        }

        path.remove(&node);
        Ok(())
    }
}

pub struct PackageResolver {
    registry: Arc<PackageRegistry>,
}

impl PackageResolver {
    pub fn new(registry: Arc<PackageRegistry>) -> Self {
        Self { registry }
    }

    pub fn resolve(
        &self,
        name: &str,
        version: &PackageVersion,
    ) -> Result<Vec<Package>, PackageError> {
        let mut resolved = HashMap::new();
        let pkg = self
            .registry
            .get(name, version)
            .ok_or_else(|| PackageError::PackageNotFound(name.to_string(), version.clone()))?;

        self.resolve_recursive(&pkg, &mut resolved)?;
        Ok(resolved.into_values().collect())
    }

    fn resolve_recursive(
        &self,
        pkg: &Package,
        resolved: &mut HashMap<(String, PackageVersion), Package>,
    ) -> Result<(), PackageError> {
        let node = (pkg.name.clone(), pkg.version.clone());
        if resolved.contains_key(&node) {
            return Ok(());
        }

        resolved.insert(node, pkg.clone());

        for dep in &pkg.dependencies {
            let dep_pkg = self.registry.get(&dep.name, &dep.version).ok_or_else(|| {
                PackageError::DependencyNotFound(dep.name.clone(), dep.version.clone())
            })?;
            self.resolve_recursive(&dep_pkg, resolved)?;
        }
        Ok(())
    }
}

pub struct PackageInstaller {
    registry: Arc<PackageRegistry>,
}

impl PackageInstaller {
    pub fn new(registry: Arc<PackageRegistry>) -> Self {
        Self { registry }
    }

    pub fn install(&self, pkg: Package) -> Result<(), PackageError> {
        PackageValidator::validate(&pkg, &self.registry)?;
        self.registry.register(pkg)?;
        Ok(())
    }
}

use axum::Router;
use std::sync::OnceLock;
use crate::api::server::ApiState;
use crate::business::runtime::CustomerInteractionRuntime;
use crate::MoatResult;

#[async_trait::async_trait]
pub trait DynamicPackage: Send + Sync {
    fn id(&self) -> &str;
    async fn initialize(&self, runtime: Arc<CustomerInteractionRuntime>) -> MoatResult<()>;
    fn register_routes(&self, router: Router<ApiState>) -> Router<ApiState> {
        router
    }
    async fn shutdown(&self) -> MoatResult<()> {
        Ok(())
    }
}

pub static DYNAMIC_PACKAGE_REGISTRY: OnceLock<parking_lot::RwLock<HashMap<String, Arc<dyn DynamicPackage>>>> = OnceLock::new();

pub fn register_dynamic_package(package: Arc<dyn DynamicPackage>) {
    let registry = DYNAMIC_PACKAGE_REGISTRY.get_or_init(|| parking_lot::RwLock::new(HashMap::new()));
    let mut map = registry.write();
    map.insert(package.id().to_string(), package);
}

pub async fn initialize_dynamic_packages(runtime: Arc<CustomerInteractionRuntime>) -> MoatResult<()> {
    if let Some(registry_lock) = DYNAMIC_PACKAGE_REGISTRY.get() {
        let registry = registry_lock.read();
        for (id, package) in registry.iter() {
            println!("[DynamicPackage] Initializing package: {}", id);
            package.initialize(runtime.clone()).await?;
        }
    }
    Ok(())
}

pub async fn shutdown_dynamic_packages() {
    if let Some(registry_lock) = DYNAMIC_PACKAGE_REGISTRY.get() {
        let mut registry = registry_lock.write();
        for (id, package) in registry.drain() {
            println!("[DynamicPackage] Shutting down package: {}", id);
            let _ = package.shutdown().await;
        }
    }
}
