use std::collections::BTreeMap;

use itertools::Either;

use uv_normalize::{GroupName, PackageName, DEV_DEPENDENCIES};
use uv_pypi_types::VerbatimParsedUrl;
use uv_resolver::Lock;
use uv_workspace::dependency_groups::{DependencyGroupError, FlatDependencyGroups};
use uv_workspace::{ProjectWorkspace, VirtualProject, Workspace};

/// A target that can be installed.
#[derive(Debug, Copy, Clone)]
pub enum InstallTarget<'env> {
    /// A project (which could be a workspace root or member).
    Project(&'env Workspace, &'env PackageName, &'env Lock),
    /// An entire workspace.
    Workspace(&'env Workspace, &'env Lock),
    /// A (legacy) workspace with a non-project root.
    NonProjectWorkspace(&'env Workspace, &'env Lock),
}

impl<'env> InstallTarget<'env> {
    /// Return the [`Workspace`] of the target.
    pub fn workspace(&self) -> &Workspace {
        match self {
            Self::Project(workspace) => workspace,
            Self::Workspace(workspace) => workspace,
            Self::NonProjectWorkspace(workspace) => workspace,
            Self::FrozenProject(workspace, _) => workspace,
            Self::FrozenWorkspace(workspace, _) => workspace,
            Self::FrozenNonProjectWorkspace(workspace, _) => workspace,
        }
    }

    /// Return the [`PackageName`] of the target.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project(project) => Either::Left(std::iter::once(project.project_name())),
            Self::Workspace(workspace) => Either::Right(workspace.packages().keys()),
            Self::NonProjectWorkspace(workspace) => Either::Right(workspace.packages().keys()),
            Self::FrozenProject(_, package_name) => Either::Left(std::iter::once(*package_name)),
            Self::FrozenWorkspace(_, lock) => Either::Right(workspace.packages().keys()),
            Self::FrozenNonProjectWorkspace(_, lock) => Either::Right(workspace.packages().keys()),
        }
    }

    /// Return the [`InstallTarget`] dependency groups.
    ///
    /// Returns dependencies that apply to the workspace root, but not any of its members. As such,
    /// only returns a non-empty iterator for virtual workspaces, which can include dev dependencies
    /// on the virtual root.
    pub fn groups(
        &self,
    ) -> Result<
        BTreeMap<GroupName, Vec<uv_pep508::Requirement<VerbatimParsedUrl>>>,
        DependencyGroupError,
    > {
        match self {
            Self::Project(_) => Ok(BTreeMap::default()),
            Self::Workspace(_) => Ok(BTreeMap::default()),
            Self::FrozenProject(_, _) => Ok(BTreeMap::default()),
            Self::FrozenWorkspace(_, _) => Ok(BTreeMap::default()),
            Self::NonProjectWorkspace(workspace) | Self::FrozenNonProjectWorkspace(workspace, _) => {
                // For non-projects, we might have `dependency-groups` or `tool.uv.dev-dependencies`
                // that are attached to the workspace root (which isn't a member).

                // First, collect `tool.uv.dev_dependencies`
                let dev_dependencies = workspace
                    .pyproject_toml()
                    .tool
                    .as_ref()
                    .and_then(|tool| tool.uv.as_ref())
                    .and_then(|uv| uv.dev_dependencies.as_ref());

                // Then, collect `dependency-groups`
                let dependency_groups = workspace
                    .pyproject_toml()
                    .dependency_groups
                    .iter()
                    .flatten()
                    .collect::<BTreeMap<_, _>>();

                // Merge any overlapping groups.
                let mut map = BTreeMap::new();
                for (name, dependencies) in
                    FlatDependencyGroups::from_dependency_groups(&dependency_groups)?
                        .into_iter()
                        .chain(
                            // Only add the `dev` group if `dev-dependencies` is defined.
                            dev_dependencies.into_iter().map(|requirements| {
                                (DEV_DEPENDENCIES.clone(), requirements.clone())
                            }),
                        )
                {
                    match map.entry(name) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(dependencies);
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            entry.get_mut().extend(dependencies);
                        }
                    }
                }

                Ok(map)
            }
        }
    }

    /// Return the [`PackageName`] of the target, if available.
    pub fn project_name(&self) -> Option<&PackageName> {
        match self {
            Self::Project(project) => Some(project.project_name()),
            Self::Workspace(_) => None,
            Self::NonProjectWorkspace(_) => None,
            Self::FrozenProject(_, package_name) => Some(package_name),
            Self::FrozenWorkspace(_, _) => None,
            Self::FrozenNonProjectWorkspace(_, _) => None,
        }
    }

    pub fn from_workspace(workspace: &'env VirtualProject) -> Self {
        match workspace {
            VirtualProject::Project(project) => Self::Workspace(project.workspace()),
            VirtualProject::NonProject(workspace) => Self::NonProjectWorkspace(workspace),
        }
    }

    pub fn from_project(project: &'env VirtualProject) -> Self {
        match project {
            VirtualProject::Project(project) => Self::Project(project),
            VirtualProject::NonProject(workspace) => Self::NonProjectWorkspace(workspace),
        }
    }
}