#[cfg(test)]
mod tests {
    use crate::v2::package::core::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_multi_version_operation() {
        let registry = Arc::new(PackageRegistry::new());
        let mut handles = vec![];

        for i in 0..10 {
            let reg_clone = Arc::clone(&registry);
            handles.push(thread::spawn(move || {
                let pkg = Package {
                    name: "my_pkg".to_string(),
                    version: PackageVersion::new(1, i, 0),
                    dependencies: vec![],
                    defines_runtime_primitives: false,
                };
                let installer = PackageInstaller::new(reg_clone);
                installer.install(pkg).unwrap();
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let versions = registry.list_versions("my_pkg");
        assert_eq!(versions.len(), 10);
        for i in 0..10 {
            assert!(versions.contains(&PackageVersion::new(1, i, 0)));
        }
    }

    #[test]
    fn test_version_pinning_enforcement() {
        let registry = Arc::new(PackageRegistry::new());
        let installer = PackageInstaller::new(Arc::clone(&registry));

        let pkg_v1 = Package {
            name: "dep".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![],
            defines_runtime_primitives: false,
        };
        let pkg_v2 = Package {
            name: "dep".to_string(),
            version: PackageVersion::new(2, 0, 0),
            dependencies: vec![],
            defines_runtime_primitives: false,
        };

        installer.install(pkg_v1).unwrap();
        installer.install(pkg_v2).unwrap();

        // App depends strictly on 1.0.0
        let app = Package {
            name: "app".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![PackageDependency {
                name: "dep".to_string(),
                version: PackageVersion::new(1, 0, 0),
            }],
            defines_runtime_primitives: false,
        };
        installer.install(app).unwrap();

        let resolver = PackageResolver::new(Arc::clone(&registry));
        let resolved = resolver
            .resolve("app", &PackageVersion::new(1, 0, 0))
            .unwrap();

        assert_eq!(resolved.len(), 2);

        let dep_resolved = resolved.iter().find(|p| p.name == "dep").unwrap();
        assert_eq!(dep_resolved.version, PackageVersion::new(1, 0, 0));

        // App2 depends on a non-existent version
        let app2 = Package {
            name: "app2".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![PackageDependency {
                name: "dep".to_string(),
                version: PackageVersion::new(1, 5, 0),
            }],
            defines_runtime_primitives: false,
        };

        let err = installer.install(app2).unwrap_err();
        match err {
            PackageError::DependencyNotFound(n, v) => {
                assert_eq!(n, "dep");
                assert_eq!(v, PackageVersion::new(1, 5, 0));
            }
            _ => panic!("Expected DependencyNotFound"),
        }
    }

    #[test]
    fn test_dependency_acyclic_validation() {
        let registry = Arc::new(PackageRegistry::new());

        // Directly register cyclic dependencies (since Installer prevents it bottom-up)
        let pkg_a = Package {
            name: "a".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![PackageDependency {
                name: "b".to_string(),
                version: PackageVersion::new(1, 0, 0),
            }],
            defines_runtime_primitives: false,
        };

        let pkg_b = Package {
            name: "b".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![PackageDependency {
                name: "a".to_string(),
                version: PackageVersion::new(1, 0, 0),
            }],
            defines_runtime_primitives: false,
        };

        registry.register(pkg_b.clone()).unwrap();
        registry.register(pkg_a.clone()).unwrap();

        let err = PackageValidator::validate(&pkg_a, &registry).unwrap_err();
        assert_eq!(err, PackageError::CyclicDependency);
    }

    #[test]
    fn test_runtime_primitive_redefinition() {
        let registry = Arc::new(PackageRegistry::new());
        let installer = PackageInstaller::new(Arc::clone(&registry));

        let pkg = Package {
            name: "hack".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![],
            defines_runtime_primitives: true,
        };

        let err = installer.install(pkg).unwrap_err();
        match err {
            PackageError::RedefinesRuntimePrimitives(n) => assert_eq!(n, "hack"),
            _ => panic!("Expected RedefinesRuntimePrimitives"),
        }
    }

    #[test]
    fn test_package_already_exists() {
        let registry = Arc::new(PackageRegistry::new());
        let installer = PackageInstaller::new(Arc::clone(&registry));

        let pkg = Package {
            name: "pkg".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![],
            defines_runtime_primitives: false,
        };

        installer.install(pkg.clone()).unwrap();
        let err = installer.install(pkg).unwrap_err();
        match err {
            PackageError::PackageAlreadyExists(n, v) => {
                assert_eq!(n, "pkg");
                assert_eq!(v, PackageVersion::new(1, 0, 0));
            }
            _ => panic!("Expected PackageAlreadyExists"),
        }
    }

    #[test]
    fn test_resolver_package_not_found() {
        let registry = Arc::new(PackageRegistry::new());
        let resolver = PackageResolver::new(Arc::clone(&registry));

        let err = resolver
            .resolve("non_existent", &PackageVersion::new(1, 0, 0))
            .unwrap_err();
        match err {
            PackageError::PackageNotFound(n, v) => {
                assert_eq!(n, "non_existent");
                assert_eq!(v, PackageVersion::new(1, 0, 0));
            }
            _ => panic!("Expected PackageNotFound"),
        }
    }

    #[test]
    fn test_resolver_dependency_not_found() {
        let registry = Arc::new(PackageRegistry::new());

        // Directly register a package with a missing dependency
        let app = Package {
            name: "app".to_string(),
            version: PackageVersion::new(1, 0, 0),
            dependencies: vec![PackageDependency {
                name: "missing".to_string(),
                version: PackageVersion::new(1, 0, 0),
            }],
            defines_runtime_primitives: false,
        };
        registry.register(app).unwrap();

        let resolver = PackageResolver::new(Arc::clone(&registry));
        let err = resolver
            .resolve("app", &PackageVersion::new(1, 0, 0))
            .unwrap_err();
        match err {
            PackageError::DependencyNotFound(n, v) => {
                assert_eq!(n, "missing");
                assert_eq!(v, PackageVersion::new(1, 0, 0));
            }
            _ => panic!("Expected DependencyNotFound"),
        }
    }
}
