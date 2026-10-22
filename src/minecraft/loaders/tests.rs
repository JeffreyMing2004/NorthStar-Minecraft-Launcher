use super::*;

#[test]
fn loader_catalogs_match_minecraft_including_new_numbering() {
    let xml = "<metadata><versioning><versions><version>21.1.1</version><version>21.0.167</version><version>26.1.2.121</version><version>26.3.0.95</version></versions></versioning></metadata>";
    for (minecraft, expected) in [
        ("1.21.1", "21.1.1"),
        ("1.21", "21.0.167"),
        ("26.1.2", "26.1.2.121"),
        ("26.3", "26.3.0.95"),
    ] {
        let versions = catalog::parse_maven(xml, minecraft, LoaderKind::NeoForge).unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, expected);
    }
    let xml = "<metadata><versioning><versions><version>1.20.1-47.4.28</version><version>1.20.2-48.0.1</version></versions></versioning></metadata>";
    assert_eq!(
        catalog::parse_maven(xml, "1.20.1", LoaderKind::Forge).unwrap()[0].version,
        "47.4.28"
    );
}

#[test]
fn maven_paths_and_install_arguments_preserve_spaces_without_shells() {
    use std::collections::BTreeMap;
    assert_eq!(
        profile::maven_path("net.test:artifact:1.2:all@zip").unwrap(),
        "net/test/artifact/1.2/artifact-1.2-all.zip"
    );
    for name in [
        "../bad:artifact:1",
        "a:b:../bad",
        "a:b:1:../../bad",
        "a:b:1@exe/evil",
        "a:b",
    ] {
        assert!(profile::maven_path(name).is_err());
    }
    let root = Path::new("game with spaces");
    let values = BTreeMap::from([
        ("SIDE".into(), "client".into()),
        ("ROOT".into(), "game with spaces".into()),
    ]);
    assert_eq!(
        processors::expand("{ROOT}/some file-{SIDE}.jar", &values, root).unwrap(),
        "game with spaces/some file-client.jar"
    );
    assert!(processors::expand("{UNKNOWN}", &values, root).is_err());
    assert_eq!(
        profile::library_key("org.ow2.asm:asm:9.1"),
        profile::library_key("org.ow2.asm:asm:9.10.1")
    );
    assert_ne!(
        profile::library_key("org.lwjgl:lwjgl:3.3.1:natives-windows"),
        profile::library_key("org.lwjgl:lwjgl:3.3.1")
    );
    let values = BTreeMap::from([("ROOT".into(), "game {with braces} and 'quotes'".into())]);
    assert_eq!(
        processors::expand("{ROOT}/client.jar", &values, root).unwrap(),
        "game {with braces} and 'quotes'/client.jar"
    );
}

#[test]
fn legacy_game_arguments_replace_parent_and_keep_required_runtime_metadata() {
    let base: Version = serde_json::from_value(serde_json::json!({
        "id":"1.12.2","type":"release","mainClass":"net.minecraft.client.main.Main",
        "downloads":{},"libraries":[
            {"name":"test:library:1","downloads":{}},
            {"name":"org.lwjgl:lwjgl:2:natives-windows","downloads":{}},
            {"name":"org.lwjgl:lwjgl:2:natives-linux","downloads":{}}
        ],
        "assetIndex":{"id":"1.12","url":"https://example.com/index.json","sha1":"0000000000000000000000000000000000000000"},
        "minecraftArguments":"--username ${auth_player_name}","javaVersion":{"majorVersion":8}
    })).unwrap();
    let child = serde_json::json!({
        "id":"native-forge-id","inheritsFrom":"1.12.2","mainClass":"net.minecraft.launchwrapper.Launch",
        "minecraftArguments":"--username ${auth_player_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
        "libraries":[{"name":"test:library:2","downloads":{}}]
    });
    let merged = profile::merge(&base, child, "1.12.2-forge-test").unwrap();
    assert_eq!(merged.java_major(), 8);
    assert_eq!(merged.libraries.len(), 3);
    assert_eq!(merged.libraries[0].name, "test:library:2");
    assert!(merged.arguments.is_none());
    assert!(merged.inherits_from.is_none());
    assert_eq!(
        merged
            .legacy_arguments
            .unwrap()
            .matches("--username")
            .count(),
        1
    );
    assert_eq!(merged.main_class, "net.minecraft.launchwrapper.Launch");
}
