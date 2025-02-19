use thiserror::Error;

use pyo3::prelude::*;

use crate::commands::check::{check_internal, CheckError};
use crate::config::edit::{ConfigEditor, EditError};
use crate::config::root_module::{RootModuleTreatment, ROOT_MODULE_SENTINEL_TAG};
use crate::config::{DependencyConfig, ProjectConfig};
use crate::diagnostics::Diagnostic;
use crate::filesystem::validate_module_path;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Failed to write project configuration to file.\n{0}")]
    FileWrite(#[from] std::io::Error),
    #[error("Failed to serialize project configuration to TOML.\n{0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("Failed to sync project.\n{0}")]
    CheckError(#[from] CheckError),
    #[error("Failed to sync project configuration due to root module violation.\n{0}")]
    RootModuleViolation(String),
    #[error("Failed to apply edits to project configuration.\n{0}")]
    EditError(#[from] EditError),
}

fn handle_added_dependency(
    module_path: &str,
    dependency: &str,
    project_config: &mut ProjectConfig,
) -> Result<(), SyncError> {
    let module_is_root = module_path == ROOT_MODULE_SENTINEL_TAG;
    let dependency_is_root = dependency == ROOT_MODULE_SENTINEL_TAG;

    if !module_is_root && !dependency_is_root {
        project_config.add_dependency(module_path.to_string(), dependency.to_string())?;
        return Ok(());
    }

    match project_config.root_module {
        RootModuleTreatment::Ignore => Ok(()),
        RootModuleTreatment::Allow => {
            project_config.add_dependency(module_path.to_string(), dependency.to_string())?;
            Ok(())
        }
        RootModuleTreatment::Forbid => Err(SyncError::RootModuleViolation(format!(
            "The root module is forbidden, but it was found that '{}' depends on '{}'.",
            module_path, dependency
        ))),
        RootModuleTreatment::DependenciesOnly => {
            if dependency_is_root {
                return Err(SyncError::RootModuleViolation(format!("No module may depend on the root module, but it was found that '{}' depends on the root module.", module_path)));
            }
            project_config.add_dependency(module_path.to_string(), dependency.to_string())?;
            Ok(())
        }
    }
}

fn detect_dependencies(diagnostics: &[Diagnostic]) -> HashMap<String, Vec<String>> {
    let mut dependencies = HashMap::new();
    for diagnostic in diagnostics {
        if diagnostic.is_dependency_error() {
            let source_path = diagnostic.usage_module().unwrap();
            let dep_path = diagnostic.definition_module().unwrap();
            dependencies
                .entry(source_path.to_string())
                .or_insert(vec![])
                .push(dep_path.to_string());
        }
    }
    dependencies
}

#[derive(Default, Clone)]
#[pyclass(get_all, module = "tach.extension")]
pub struct UnusedDependencies {
    pub path: String,
    pub dependencies: Vec<DependencyConfig>,
}

pub fn detect_unused_dependencies(
    project_root: PathBuf,
    project_config: &ProjectConfig,
) -> Result<Vec<UnusedDependencies>, SyncError> {
    // This is a shortcut to finding all cross-module dependencies
    // TODO: dedicated function
    let cleared_project_config = project_config.with_dependencies_removed();
    let check_result = check_internal(project_root, &cleared_project_config, true, false)?;
    let detected_dependencies = detect_dependencies(&check_result);

    let mut unused_dependencies: Vec<UnusedDependencies> = vec![];
    for module_path in project_config
        .module_paths()
        .into_iter()
        // This is a hack to avoid checking globbed modules for unused dependencies
        .filter(|path| !path.contains("*"))
    {
        let module_detected_dependencies =
            detected_dependencies
                .get(&module_path)
                .map_or(HashSet::new(), |deps| {
                    deps.iter()
                        .map(|dep| dep.to_string())
                        .collect::<HashSet<_>>()
                });

        // Get current dependencies for the module
        let current_deps = project_config
            .dependencies_for_module(&module_path)
            .map(|deps| deps.to_vec())
            .unwrap_or_default();

        // Find dependencies that don't match any detected paths
        let unused_deps: Vec<DependencyConfig> = current_deps
            .into_iter()
            .filter(|dep| {
                !module_detected_dependencies
                    .iter()
                    .any(|detected| dep.matches(detected))
            })
            .collect();

        if !unused_deps.is_empty() {
            unused_dependencies.push(UnusedDependencies {
                path: module_path.to_string(),
                dependencies: unused_deps,
            });
        }
    }

    Ok(unused_dependencies)
}

fn sync_dependency_constraints(
    project_root: PathBuf,
    project_config: &mut ProjectConfig,
    prune: bool,
) -> Result<(), SyncError> {
    // This is a shortcut to finding all cross-module dependencies
    // TODO: dedicated function
    let cleared_project_config = project_config.with_dependencies_removed();
    let check_result = check_internal(project_root, &cleared_project_config, true, false)?;
    let detected_dependencies = detect_dependencies(&check_result);

    // Root module is a special case -- it may not be in module paths and still implicitly detect dependencies
    // If the root module is not in the module paths, but was detected, create it
    if !project_config
        .module_paths()
        .contains(&ROOT_MODULE_SENTINEL_TAG.to_string())
        && (detected_dependencies.contains_key(ROOT_MODULE_SENTINEL_TAG)
            || detected_dependencies
                .values()
                .any(|deps| deps.contains(&ROOT_MODULE_SENTINEL_TAG.to_string())))
    {
        // This enqueues an edit to the TOML
        project_config.create_module(ROOT_MODULE_SENTINEL_TAG.to_string())?;
        // This adds the root module to the module paths immediately
        project_config.add_root_module();
    }

    // Now diff with project config and apply edits
    for module_path in project_config
        .module_paths()
        .into_iter()
        // This is a hack to avoid attempting to sync globbed modules
        .filter(|path| !path.contains("*"))
    {
        let module_detected_dependencies =
            detected_dependencies
                .get(&module_path)
                .map_or(HashSet::new(), |deps| {
                    deps.iter()
                        .map(|dep| dep.to_string())
                        .collect::<HashSet<_>>()
                });

        // Get current dependencies for the module
        let current_deps = project_config
            .dependencies_for_module(&module_path)
            .map(|deps| deps.to_vec())
            .unwrap_or_default();

        // Find detected dependencies that don't match any current dependency patterns
        let deps_to_add: Vec<String> = module_detected_dependencies
            .iter()
            .filter(|detected| !current_deps.iter().any(|dep| dep.matches(detected)))
            .cloned()
            .collect();

        // Add new dependencies
        for dep in deps_to_add {
            handle_added_dependency(&module_path, &dep, project_config)?;
        }

        if prune {
            // Find current dependencies that don't match any detected paths
            let deps_to_remove: Vec<String> = current_deps
                .iter()
                .filter(|dep| {
                    !module_detected_dependencies
                        .iter()
                        .any(|detected| dep.matches(detected))
                })
                .map(|dep| dep.path.clone())
                .collect();

            for dep in deps_to_remove {
                project_config.remove_dependency(module_path.to_string(), dep)?;
            }
        }
    }

    if prune {
        project_config
            .module_paths()
            .iter()
            // This is a hack to avoid pruning globbed modules
            .filter(|path| !path.contains("*"))
            .for_each(|module_path| {
                if !validate_module_path(
                    &project_config.absolute_source_roots().unwrap(),
                    module_path,
                ) {
                    // Not clear what to do if enqueueing deletion fails
                    let _ = project_config.delete_module(module_path.to_string());
                }
            });
    }

    Ok(())
}

/// Update project configuration with auto-detected dependency constraints.
/// If prune is set to False, it will create dependencies to resolve existing errors,
/// but will not remove any constraints.
pub fn sync_project(
    project_root: PathBuf,
    mut project_config: ProjectConfig,
    add: bool,
) -> Result<(), SyncError> {
    // This may queue edits to the project config
    sync_dependency_constraints(project_root, &mut project_config, !add)?;

    project_config.apply_edits()?;

    Ok(())
}
