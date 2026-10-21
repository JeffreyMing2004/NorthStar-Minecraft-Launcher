mod download;
mod install;
mod launch;
mod model;
mod source;

pub use download::{Downloader, Progress, Reporter};
pub use install::{InstalledVersion, install, installed_versions, manifest};
pub use launch::{LaunchPlan, launch, prepare_launch, validate_player_name};
pub use model::{VersionEntry, VersionManifest};
pub use source::{Source, SourceKind};

pub type Result<T> = std::result::Result<T, String>;

#[cfg(test)]
mod tests;
