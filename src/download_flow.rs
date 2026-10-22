use crate::minecraft::{LoaderKind, LoaderVersion, VersionEntry};

#[derive(Default)]
pub struct DownloadFlow {
    pub version: Option<VersionEntry>,
    pub kind: Option<LoaderKind>,
    pub loader: Option<LoaderVersion>,
}

pub enum InstallTarget {
    Vanilla(VersionEntry),
    Modded(LoaderVersion),
}

impl InstallTarget {
    pub fn id(&self) -> Option<String> {
        match self {
            Self::Vanilla(version) => Some(version.id.clone()),
            Self::Modded(loader) => loader.id().ok(),
        }
    }
}

impl DownloadFlow {
    pub fn choose_version(&mut self, version: VersionEntry) {
        *self = Self {
            version: Some(version),
            ..Default::default()
        };
    }

    pub fn choose_kind(&mut self, kind: Option<LoaderKind>) {
        if self.kind != kind {
            self.kind = kind;
            self.loader = None;
        }
    }

    pub fn choose_loader(&mut self, loader: LoaderVersion) {
        if self.kind == Some(loader.kind)
            && self
                .version
                .as_ref()
                .is_some_and(|v| v.id == loader.minecraft)
        {
            self.loader = Some(loader);
        }
    }

    pub fn target(&self) -> Option<InstallTarget> {
        let version = self.version.as_ref()?;
        match self.kind {
            None => Some(InstallTarget::Vanilla(version.clone())),
            Some(kind) => self
                .loader
                .as_ref()
                .filter(|loader| loader.kind == kind && loader.minecraft == version.id)
                .cloned()
                .map(InstallTarget::Modded),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minecraft(id: &str) -> VersionEntry {
        VersionEntry {
            id: id.into(),
            kind: "release".into(),
            url: String::new(),
            sha1: String::new(),
            release_time: String::new(),
        }
    }

    fn loader(kind: LoaderKind, minecraft: &str) -> LoaderVersion {
        LoaderVersion {
            kind,
            minecraft: minecraft.into(),
            version: "test".into(),
            stable: true,
        }
    }

    #[test]
    fn vanilla_is_optional_and_each_loader_requires_a_compatible_version() {
        let mut flow = DownloadFlow::default();
        assert!(flow.target().is_none());
        flow.choose_version(minecraft("1.20.1"));
        assert!(matches!(flow.target(), Some(InstallTarget::Vanilla(_))));
        for kind in LoaderKind::ALL {
            flow.choose_kind(Some(kind));
            assert!(flow.target().is_none());
            flow.choose_loader(loader(kind, "1.21.1"));
            assert!(flow.target().is_none());
            flow.choose_loader(loader(kind, "1.20.1"));
            assert!(matches!(flow.target(), Some(InstallTarget::Modded(_))));
        }
        flow.choose_kind(None);
        assert!(flow.loader.is_none());
        assert_eq!(flow.target().unwrap().id().as_deref(), Some("1.20.1"));
    }

    #[test]
    fn changing_minecraft_or_loader_cannot_reuse_a_previous_selection() {
        let mut flow = DownloadFlow::default();
        flow.choose_version(minecraft("1.20.1"));
        flow.choose_kind(Some(LoaderKind::Forge));
        flow.choose_loader(loader(LoaderKind::Forge, "1.20.1"));
        flow.choose_kind(Some(LoaderKind::Fabric));
        assert!(flow.loader.is_none());
        flow.choose_loader(loader(LoaderKind::Forge, "1.20.1"));
        assert!(flow.target().is_none());
        flow.choose_loader(loader(LoaderKind::Fabric, "1.20.1"));
        flow.choose_version(minecraft("1.21.1"));
        assert!(flow.kind.is_none());
        assert!(flow.loader.is_none());
        assert_eq!(flow.target().unwrap().id().as_deref(), Some("1.21.1"));
    }
}
