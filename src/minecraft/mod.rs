mod download;
mod install;
mod launch;
mod loaders;
mod model;
mod source;

pub use download::validate_id;
pub use download::{Downloader, Progress, Reporter};
pub use install::hidden_command;
pub use install::{InstalledVersion, install, installed_versions, manifest};
pub use launch::{
    LaunchIdentity, LaunchPlan, launch, prepare_instance_launch, prepare_launch,
    validate_player_name,
};
pub use loaders::{LoaderKind, LoaderVersion, install_loader, loader_versions};
pub use model::{VersionEntry, VersionManifest};
pub use source::{Source, SourceKind};

pub type Result<T> = std::result::Result<T, String>;

#[cfg(test)]
mod tests;
