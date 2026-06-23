use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, RwLock};
use thiserror::Error;

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
    pub version: PackageVersion, // Strict version pinning
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
