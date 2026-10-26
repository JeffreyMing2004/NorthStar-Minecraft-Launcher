mod download;
pub(crate) mod external;
mod install;
mod launch;
mod loaders;
mod logging;
mod model;
mod source;

pub use download::validate_id;
pub use download::validate_local_id;
pub use download::{Downloader, Progress, Reporter};
pub(crate) use download::{Job, safe_join};
pub use install::hidden_command;
pub use install::{InstalledVersion, install, installed_versions, manifest};
pub use launch::{
    LaunchIdentity, LaunchPlan, launch, offline_uuid, prepare_instance_launch,
    prepare_instance_launch_with_downloader, prepare_launch, validate_player_name,
};
pub use loaders::{LoaderKind, LoaderVersion, install_loader, loader_versions};
pub use model::{VersionEntry, VersionManifest};
pub use source::{ProxyMode, Source, SourceKind};

pub type Result<T> = std::result::Result<T, String>;

#[cfg(test)]
mod tests;
