use std::collections::BTreeMap;

use either::Either;

use uv_normalize::{GroupName, PackageName, DEV_DEPENDENCIES};
use uv_pypi_types::VerbatimParsedUrl;
use uv_workspace::dependency_groups::{DependencyGroupError, FlatDependencyGroups};
use uv_workspace::Workspace;

use crate::Lock;

/// A target that can be installed.
#[derive(Debug, Copy, Clone)]
pub enum InstallTarget<'env> {
    /// A project (which could be a workspace root or member).
    Project { workspace: &'env Workspace, name: &'env PackageName, lock: &'env Lock },
    /// An entire workspace.
    Workspace { workspace: &'env Workspace, lock: &'env Lock },
    /// A (legacy) workspace with a non-project root.
    NonProjectWorkspace { workspace: &'env Workspace, lock: &'env Lock},
}

impl<'env> InstallTarget<'env> {
    /// Return the [`Workspace`] of the target.
    pub fn workspace(&self) -> &Workspace {
        match self {
            Self::Project { workspace, ..} => workspace,
            Self::Workspace { workspace, ..} => workspace,
            Self::NonProjectWorkspace { workspace, ..} => workspace,
        }
    }

    /// Return the [`PackageName`] of the target.
    pub fn packages(&self) -> impl Iterator<Item = &PackageName> {
        match self {
            Self::Project { name, ..} => Either::Right(Either::Left(std::iter::once(*name))),
            Self::NonProjectWorkspace { lock, .. } => {
                Either::Left(lock.members().into_iter())
            }
            Self::Workspace { lock, .. } => {
                // Identify the workspace members.
                //
                // The members are encoded directly in the lockfile, unless the workspace contains a
                // single member at the root, in which case, we identify it by its source.
                if lock.members().is_empty() {
                    Either::Right(Either::Right(lock.root().into_iter()))
                } else {
                    Either::Left(lock.members().into_iter())
                }
            },
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
            Self::Project { .. } => Ok(BTreeMap::default()),
            Self::Workspace { .. } => Ok(BTreeMap::default()),
            Self::NonProjectWorkspace { workspace, ..  }=> {
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
            Self::Project { name, ..} => Some(name),
            Self::Workspace {.. } => None,
            Self::NonProjectWorkspace {.. } => None,
        }
    }
}