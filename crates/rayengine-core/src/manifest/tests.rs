use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rayengine-manifest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn load(&self, source: &str) -> Result<ProjectManifest> {
        let path = self.0.join(FILE_NAME);
        fs::write(&path, source).unwrap();
        ProjectManifest::load(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn minimal_manifest_defaults_and_optional_workflow() {
    let dir = Scratch::new();
    assert!(ProjectManifest::load_optional(&dir.0).unwrap().is_none());
    assert!(ProjectManifest::load_optional(dir.0.join(FILE_NAME)).is_err());
    assert!(ProjectManifest::load_optional(dir.0.join("missing")).is_err());
    let project = dir
        .load("schema_version = 1")
        .unwrap()
        .resolve(None)
        .unwrap();
    let mut expected = Settings::default();
    expected.assets.roots = vec![dir.0.join("assets")];
    assert_eq!(project.settings, expected);
    assert!(!project.is_declared("window", "size"));
    assert_eq!(project.path, dir.0.join(FILE_NAME).canonicalize().unwrap());
    assert!(project.discover_assets().unwrap().is_empty());
}

#[test]
fn profiles_merge_tables_replace_arrays_and_keep_extensions() {
    let dir = Scratch::new();
    let manifest = dir
        .load(
            r#"
schema_version = 1
[assets]
roots = ["shared", "second"]
exclude = ["*.bak"]
[window]
size = [800, 600]
[fonts.ui]
path = "font.ttf"
glyphs = [32, 65]
[game.nested]
keep = 1
replace = [1, 2]
[plugins.weather]
future_setting = { value = true }
[package]
future_setting = 3
[profiles.dev.assets]
roots = ["dev"]
[profiles.dev.window]
title = "Development"
[profiles.dev.fonts.ui]
filter = "nearest"
glyphs = [66]
[profiles.dev.game.nested]
replace = [3]
[profiles.other.window]
title = "Other"
"#,
        )
        .unwrap();
    let dev = manifest.resolve(Some("dev")).unwrap();
    assert_eq!(dev.settings.window.size, [800, 600]);
    assert_eq!(dev.settings.window.title, "Development");
    assert_eq!(dev.settings.assets.roots, [dir.0.join("dev")]);
    assert_eq!(dev.settings.assets.exclude, ["*.bak"]);
    assert_eq!(dev.settings.fonts["ui"].path, dir.0.join("font.ttf"));
    assert_eq!(dev.settings.fonts["ui"].filter, FontFilter::Nearest);
    assert_eq!(dev.settings.fonts["ui"].glyphs, Some(vec![66]));
    let nested = dev.settings.game["nested"].as_table().unwrap();
    assert_eq!(nested["keep"].as_integer(), Some(1));
    assert_eq!(
        nested["replace"].as_array().unwrap(),
        &[toml::Value::Integer(3)]
    );
    assert!(dev.settings.plugins["weather"]["future_setting"].is_table());
    assert_eq!(dev.settings.package["future_setting"].as_integer(), Some(3));
    assert_eq!(
        manifest
            .resolve(Some("other"))
            .unwrap()
            .settings
            .assets
            .roots,
        [dir.0.join("shared"), dir.0.join("second")]
    );
    assert!(
        manifest
            .resolve(Some("missing"))
            .unwrap_err()
            .to_string()
            .contains("unknown profile")
    );
    assert_eq!(manifest.profiles().collect::<Vec<_>>(), ["dev", "other"]);
}

#[test]
fn versions_unknown_keys_and_bad_values_report_manifest_context() {
    let dir = Scratch::new();
    for (source, expected) in [
        ("", "schema_version"),
        ("schema_version = 2", "schema_version"),
        ("schema_version = '1'", "schema_version"),
        ("schema_version = 1\nwindwo = {}", "windwo"),
        (
            "schema_version = 1\n[window]\nsize = [0, 600]",
            "window.size",
        ),
        (
            "schema_version = 1\n[render]\nreference_size = [nan, 600.0]",
            "render.reference_size",
        ),
        (
            "schema_version = 1\n[render]\ntarget_fps = 1001",
            "render.target_fps",
        ),
        (
            "schema_version = 1\n[render]\nscale_mode = 'unknown'",
            "unknown",
        ),
        (
            "schema_version = 1\n[runtime]\nfixed_hz = 0",
            "runtime.fixed_hz",
        ),
        (
            "schema_version = 1\n[window]\ntitle = \"\\u0000\"",
            "window.title",
        ),
        ("schema_version = 1\n[assets]\nroots = ['']", "assets.roots"),
        (
            "schema_version = 1\n[assets]\nexclude = ['[']",
            "assets.exclude",
        ),
        (
            "schema_version = 1\n[assets]\nexclude = ['../*']",
            "assets.exclude",
        ),
        (
            "schema_version = 1\n[fonts.ui]\npath = 'ui.ttf'\nraster_size = 0",
            "fonts.ui.raster_size",
        ),
        (
            "schema_version = 1\n[fonts.ui]\npath = 'ui.ttf'\nglyphs = [55296]",
            "fonts.ui.glyphs",
        ),
        ("schema_version = 1\n[plugins]\nweather = 1", "invalid type"),
        (
            "schema_version = 1\n[profiles.bad.window]\nunknown = true",
            "profiles.bad",
        ),
        (
            "schema_version = 1\n[profiles.bad.runtime]\nmax_catch_up = 0",
            "profiles.bad",
        ),
        (
            "schema_version = 1\n[profiles.bad.project]\nname = 'new'",
            "identity",
        ),
        (
            "schema_version = 1\n[profiles.bad.fonts.ui]\nfilter = 'nearest'",
            "path",
        ),
    ] {
        let message = dir.load(source).unwrap_err().to_string();
        assert!(message.contains(FILE_NAME), "{message}");
        assert!(
            message.contains(expected),
            "expected {expected:?}: {message}"
        );
    }
}

#[test]
fn manifest_relative_paths_do_not_require_files_or_depend_on_cwd() {
    let dir = Scratch::new();
    fs::create_dir(dir.0.join("project")).unwrap();
    let file = dir.0.join("project").join(FILE_NAME);
    let absolute = dir.0.join("external");
    fs::write(&file, format!("schema_version = 1\n[assets]\nroots = ['../assets', '{}']\n[fonts.ui]\npath = '../fonts/ui.ttf'", absolute.display())).unwrap();
    let from_file = ProjectManifest::load(&file).unwrap().resolve(None).unwrap();
    let from_dir = ProjectManifest::load_optional(file.parent().unwrap())
        .unwrap()
        .unwrap()
        .resolve(None)
        .unwrap();
    let from_cargo = ProjectManifest::load_optional(file.with_file_name("Cargo.toml"))
        .unwrap()
        .unwrap()
        .resolve(None)
        .unwrap();
    assert_eq!(from_file.settings, from_dir.settings);
    assert_eq!(from_file.settings, from_cargo.settings);
    assert_eq!(
        from_file.settings.assets.roots,
        [dir.0.join("project/../assets"), absolute]
    );
    assert_eq!(
        from_file.settings.fonts["ui"].path,
        dir.0.join("project/../fonts/ui.ttf")
    );
}

#[test]
fn discovery_and_lookup_agree_on_roots_exclusions_and_duplicates() {
    let dir = Scratch::new();
    for path in ["first/nested", "first/private", "second"] {
        fs::create_dir_all(dir.0.join(path)).unwrap();
    }
    for path in [
        "first/a.png",
        "first/nested/b.png",
        "first/nested/b.bak",
        "first/c.bak",
        "first/private/key",
        "second/a.png",
        "second/z.png",
    ] {
        fs::write(dir.0.join(path), "asset").unwrap();
    }
    let project = dir.load("schema_version = 1\n[assets]\nroots = ['first', 'second', 'absent']\nexclude = ['**/*.bak', 'private']").unwrap().resolve(None).unwrap();
    assert_eq!(
        project.discover_assets().unwrap(),
        [
            dir.0.join("first/a.png"),
            dir.0.join("first/nested/b.png"),
            dir.0.join("second/z.png")
        ]
    );
    assert_eq!(project.asset("a.png").unwrap(), dir.0.join("first/a.png"));
    assert_eq!(project.asset("z.png").unwrap(), dir.0.join("second/z.png"));
    for path in [
        "nested/b.bak",
        "c.bak",
        "private/key",
        "../a.png",
        "/a.png",
        "missing",
        "",
    ] {
        assert!(project.asset(path).is_err(), "{path}");
    }
    assert!(
        dir.load("schema_version = 1\n[assets]\nroots = []")
            .unwrap()
            .resolve(None)
            .unwrap()
            .discover_assets()
            .unwrap()
            .is_empty()
    );
    fs::write(dir.0.join("not-a-directory"), "file").unwrap();
    assert!(
        dir.load("schema_version = 1\n[assets]\nroots = ['not-a-directory']")
            .unwrap()
            .resolve(None)
            .unwrap()
            .discover_assets()
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn discovery_and_lookup_skip_links_and_dangling_manifest_links_fail() {
    use std::os::unix::fs::symlink;
    let dir = Scratch::new();
    fs::create_dir(dir.0.join("assets")).unwrap();
    fs::write(dir.0.join("assets/file"), "data").unwrap();
    symlink("file", dir.0.join("assets/link")).unwrap();
    symlink(".", dir.0.join("assets/loop")).unwrap();
    let project = dir
        .load("schema_version = 1")
        .unwrap()
        .resolve(None)
        .unwrap();
    assert_eq!(
        project.discover_assets().unwrap(),
        [dir.0.join("assets/file")]
    );
    assert!(project.asset("link").is_err());
    assert!(project.asset("loop/file").is_err());
    fs::remove_file(dir.0.join(FILE_NAME)).unwrap();
    symlink("absent", dir.0.join(FILE_NAME)).unwrap();
    assert!(ProjectManifest::load_optional(&dir.0).is_err());
}

#[test]
fn equivalent_asset_names_cannot_bypass_exclusions() {
    let dir = Scratch::new();
    fs::create_dir_all(dir.0.join("assets/nested")).unwrap();
    fs::write(dir.0.join("assets/nested/secret.txt"), "excluded").unwrap();
    fs::write(dir.0.join("assets/nested/public.txt"), "selected").unwrap();
    let project = dir
        .load("schema_version = 1\n[assets]\nexclude = ['nested/secret.txt']")
        .unwrap()
        .resolve(None)
        .unwrap();
    assert_eq!(
        project.discover_assets().unwrap(),
        [dir.0.join("assets/nested/public.txt")]
    );
    for name in [
        "nested/secret.txt",
        "nested/./secret.txt",
        "nested//secret.txt",
        "./nested/secret.txt",
        "nested/secret.txt/",
    ] {
        assert!(project.asset(name).is_err(), "{name}");
    }
    for name in [
        "nested/public.txt",
        "nested/./public.txt",
        "nested//public.txt",
        "./nested/public.txt",
    ] {
        assert_eq!(
            project.asset(name).unwrap(),
            dir.0.join("assets/nested/public.txt")
        );
    }
    assert!(project.asset(".").is_err());
}

#[test]
fn an_intermediate_file_does_not_block_later_asset_roots() {
    let dir = Scratch::new();
    fs::create_dir(dir.0.join("first")).unwrap();
    fs::create_dir_all(dir.0.join("second/nested")).unwrap();
    fs::write(dir.0.join("first/nested"), "regular file").unwrap();
    fs::write(dir.0.join("second/nested/file.txt"), "asset").unwrap();
    let project = dir
        .load("schema_version = 1\n[assets]\nroots = ['first', 'second']")
        .unwrap()
        .resolve(None)
        .unwrap();
    assert_eq!(
        project.discover_assets().unwrap(),
        [
            dir.0.join("first/nested"),
            dir.0.join("second/nested/file.txt")
        ]
    );
    assert_eq!(
        project.asset("nested/file.txt").unwrap(),
        dir.0.join("second/nested/file.txt")
    );
    assert_eq!(project.asset("nested").unwrap(), dir.0.join("first/nested"));
}

#[test]
fn render_quality_profiles_validate_settings_combinations_and_allocations() {
    let dir = Scratch::new();
    let manifest = dir
        .load(
            r#"
schema_version = 1
[render]
render_scale = 1.0
anti_aliasing = "none"
[profiles.smooth.render]
anti_aliasing = "fxaa"
[profiles.ultra.render]
render_scale = 2.0
anti_aliasing = "fxaa"
"#,
        )
        .unwrap();
    let native = manifest.resolve(None).unwrap();
    assert_eq!(native.settings.render.render_scale, 1.0);
    assert_eq!(
        native.settings.render.anti_aliasing,
        crate::quality::AntiAliasing::None
    );
    let smooth = manifest.resolve(Some("smooth")).unwrap();
    assert_eq!(smooth.settings.render.render_scale, 1.0);
    assert_eq!(
        smooth.settings.render.anti_aliasing,
        crate::quality::AntiAliasing::Fxaa
    );
    let ultra = manifest.resolve(Some("ultra")).unwrap();
    assert_eq!(ultra.settings.render.render_scale, 2.0);
    assert_eq!(
        ultra.settings.render.anti_aliasing,
        crate::quality::AntiAliasing::Fxaa
    );
    assert!(ultra.is_declared("render", "render_scale"));
    for (source, expected) in [
        ("[render]\nanti_aliasing = 'msaa'", "msaa"),
        ("[render]\nrender_scale = 1.5", "render_scale"),
        ("[render]\nrender_scale = nan", "render_scale"),
        (
            "[render]\nscale_mode = 'integer_fit'\nanti_aliasing = 'fxaa'",
            "IntegerFit",
        ),
        (
            "[render]\nscale_mode = 'integer_fit'\nrender_scale = 2.0",
            "IntegerFit",
        ),
        (
            "[window]\nsize = [4096,4096]\n[render]\nreference_size = [1.0,1.0]\nrender_scale = 2.0",
            "bytes",
        ),
        (
            "[profiles.bad.render]\nanti_aliasing = 'unknown'",
            "unknown",
        ),
    ] {
        let error = dir
            .load(&format!("schema_version = 1\n{source}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}
