//! Discovers plugin manifests and evaluates serializable TypeScript definitions.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};

use super::{
    definition::PluginDefinitionLoader,
    descriptor::PluginModuleDefinition,
    manifest::{validate_id, PluginManifest},
};
use crate::{config, Error, Result};

const MANIFEST_FILE_NAME: &str = "plugin.json";
const MAX_ICON_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct PluginCatalog {
    roots: Vec<PathBuf>,
    definition_loader: PluginDefinitionLoader,
    app_version: String,
}

#[derive(Clone)]
pub(crate) struct PluginEntry {
    pub directory: PathBuf,
    pub entry: PathBuf,
    pub manifest: PluginManifest,
    pub definition: PluginModuleDefinition,
    pub icon: String,
}

impl PluginCatalog {
    pub fn managed(app_version: String) -> Result<Self> {
        let installed = config::managed_data_dir()?.join("plugins/installed");
        fs::create_dir_all(&installed)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&installed, fs::Permissions::from_mode(0o700))?;
        }
        // 内置插件按版本预装进 installed;版本一致时不写盘。
        super::builtin::install(&installed)?;
        // 扫描顺序即优先级:debug 下源码目录优先,保证内置插件热改生效;
        // 发布构建只有 installed 一个根。
        #[cfg(debug_assertions)]
        let roots = vec![
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/build-in"),
            installed,
        ];
        #[cfg(not(debug_assertions))]
        let roots = vec![installed];
        Ok(Self {
            roots,
            definition_loader: PluginDefinitionLoader::managed()?,
            app_version,
        })
    }

    pub(crate) fn loader(&self) -> &PluginDefinitionLoader {
        &self.definition_loader
    }

    pub(crate) async fn entries(&self, executable: &Path) -> Vec<PluginEntry> {
        let mut plugins = BTreeMap::new();
        for root in &self.roots {
            let mut directories = match child_directories(root) {
                Ok(value) => value,
                Err(error) => {
                    tracing::warn!(path = %root.display(), %error, "failed to scan plugin directory");
                    continue;
                }
            };
            directories.sort();
            for directory in directories {
                match load_plugin(
                    &directory,
                    &self.definition_loader,
                    executable,
                    &self.app_version,
                )
                .await
                {
                    Ok(entry) => {
                        if plugins.contains_key(&entry.manifest.id) {
                            tracing::warn!(plugin = %entry.manifest.id, path = %directory.display(), "ignoring duplicate plugin");
                        } else {
                            plugins.insert(entry.manifest.id.clone(), entry);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(path = %directory.display(), %error, "ignoring invalid plugin")
                    }
                }
            }
        }
        plugins.into_values().collect()
    }

    pub(crate) fn manifests(&self) -> Vec<(PluginManifest, String)> {
        let mut plugins = BTreeMap::new();
        for root in &self.roots {
            let Ok(mut directories) = child_directories(root) else {
                continue;
            };
            directories.sort();
            for directory in directories {
                let loaded = (|| -> Result<_> {
                    let manifest: PluginManifest =
                        serde_json::from_slice(&fs::read(directory.join(MANIFEST_FILE_NAME))?)?;
                    manifest.validate(&directory)?;
                    require_app_version(&manifest, &self.app_version)?;
                    let icon = icon_data_url(&directory, &manifest.icon)?;
                    Ok((manifest, icon))
                })();
                if let Ok((manifest, icon)) = loaded {
                    plugins
                        .entry(manifest.id.clone())
                        .or_insert((manifest, icon));
                }
            }
        }
        plugins.into_values().collect()
    }
}

fn child_directories(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut directories = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
            directories.push(entry.path());
        }
    }
    Ok(directories)
}

/// 应用过旧时拒绝加载,让插件的 minAppVersion 声明生效。
fn require_app_version(manifest: &PluginManifest, app_version: &str) -> Result<()> {
    let Some(minimum) = &manifest.min_app_version else {
        return Ok(());
    };
    if super::manifest::version_at_least(app_version, minimum) {
        return Ok(());
    }
    Err(Error::Config(format!(
        "plugin '{}' requires app version {minimum} or newer (current {app_version})",
        manifest.id
    )))
}

async fn load_plugin(
    directory: &Path,
    loader: &PluginDefinitionLoader,
    executable: &Path,
    app_version: &str,
) -> Result<PluginEntry> {
    let manifest: PluginManifest =
        serde_json::from_slice(&fs::read(directory.join(MANIFEST_FILE_NAME))?)?;
    manifest.validate(directory)?;
    require_app_version(&manifest, app_version)?;
    let icon = icon_data_url(directory, &manifest.icon)?;
    let entry = directory.join(&manifest.entry).canonicalize()?;
    let definition = loader.load(executable, directory, &entry).await?;
    validate_definition(&manifest.id, &definition)?;
    Ok(PluginEntry {
        directory: directory.to_path_buf(),
        entry,
        manifest,
        definition,
        icon,
    })
}

/// 显示文本必须是非空字符串,或全为非空字符串的 locale 映射。
fn validate_localized_text(value: &serde_json::Value, label: &str) -> Result<()> {
    match value {
        serde_json::Value::String(text) if !text.trim().is_empty() => Ok(()),
        serde_json::Value::Object(map)
            if !map.is_empty()
                && map
                    .values()
                    .all(|entry| entry.as_str().is_some_and(|text| !text.trim().is_empty())) =>
        {
            Ok(())
        }
        _ => Err(Error::Config(format!(
            "{label} must be a non-empty string or a locale map of non-empty strings"
        ))),
    }
}

fn validate_definition(plugin_id: &str, definition: &PluginModuleDefinition) -> Result<()> {
    if definition.providers.is_empty() {
        return Err(Error::Config(format!(
            "plugin '{plugin_id}' must define at least one provider"
        )));
    }
    let mut provider_ids = std::collections::HashSet::new();
    for provider in &definition.providers {
        validate_id(&provider.id, "plugin provider id")?;
        validate_localized_text(
            &provider.display_name,
            &format!(
                "plugin '{plugin_id}' provider '{}' displayName",
                provider.id
            ),
        )?;
        if provider.provider_type.trim().is_empty() {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' provider '{}' requires providerType",
                provider.id
            )));
        }
        if !matches!(
            provider.catalog.as_str(),
            super::descriptor::SNAPSHOT_CATALOG | super::descriptor::LIVE_CATALOG
        ) {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' provider '{}' uses unsupported catalog '{}'",
                provider.id, provider.catalog
            )));
        }
        if provider.catalog == super::descriptor::LIVE_CATALOG {
            if !provider.has_models {
                return Err(Error::Config(format!(
                    "plugin '{plugin_id}' provider '{}' live catalog requires models",
                    provider.id
                )));
            }
            if provider.resource_type.is_none() {
                return Err(Error::Config(format!(
                    "plugin '{plugin_id}' provider '{}' live catalog requires a resource type",
                    provider.id
                )));
            }
        }
        if !provider_ids.insert(provider.id.clone()) {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' contains duplicate provider '{}'",
                provider.id
            )));
        }
        if let Some(resource_type) = &provider.resource_type {
            if !definition
                .resources
                .iter()
                .any(|resource| &resource.resource_type == resource_type)
            {
                return Err(Error::Config(format!(
                    "plugin '{plugin_id}' provider '{}' consumes undeclared resource '{resource_type}'",
                    provider.id
                )));
            }
        }
    }
    let mut resource_types = std::collections::HashSet::new();
    for resource in &definition.resources {
        validate_id(&resource.resource_type, "plugin resource type")?;
        validate_localized_text(
            &resource.display_name,
            &format!(
                "plugin '{plugin_id}' resource '{}' displayName",
                resource.resource_type
            ),
        )?;
        if !resource_types.insert(resource.resource_type.clone()) {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' contains duplicate resource type '{}'",
                resource.resource_type
            )));
        }
        for method in &resource.add {
            validate_id(&method.id, "plugin add method id")?;
            match method.method_type.as_str() {
                super::descriptor::OAUTH2_ADD_METHOD
                | super::descriptor::OAUTH2_AUTHORIZATION_CODE_ADD_METHOD
                | super::descriptor::FORM_ADD_METHOD => {}
                other => {
                    return Err(Error::Config(format!(
                        "plugin '{plugin_id}' add method '{}' uses unsupported type '{other}'",
                        method.id
                    )));
                }
            }
            if method.method_type == super::descriptor::FORM_ADD_METHOD {
                if method.callback.is_some() {
                    return Err(Error::Config(format!(
                        "plugin '{plugin_id}' form method '{}' cannot declare callback settings",
                        method.id
                    )));
                }
                validate_form_fields(plugin_id, &method.id, &method.fields)?;
            } else if !method.fields.is_empty() {
                return Err(Error::Config(format!(
                    "plugin '{plugin_id}' OAuth method '{}' cannot declare form fields",
                    method.id
                )));
            }
            if method.method_type == super::descriptor::OAUTH2_ADD_METHOD
                && method.callback.is_some()
            {
                return Err(Error::Config(format!(
                    "plugin '{plugin_id}' device OAuth method '{}' cannot declare callback settings",
                    method.id
                )));
            }
            if let Some(callback) = &method.callback {
                if callback.port == Some(0) {
                    return Err(Error::Config(format!(
                        "plugin '{plugin_id}' OAuth callback port must be greater than zero"
                    )));
                }
                if let Some(path) = callback.path.as_deref() {
                    if !path.starts_with('/')
                        || path.len() > 128
                        || path.contains('?')
                        || path.contains('#')
                        || path.contains("//")
                    {
                        return Err(Error::Config(format!(
                            "plugin '{plugin_id}' contains invalid OAuth callback path '{path}'"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_form_fields(
    plugin_id: &str,
    method_id: &str,
    fields: &[super::descriptor::FormFieldDefinition],
) -> Result<()> {
    if fields.is_empty() {
        return Err(Error::Config(format!(
            "plugin '{plugin_id}' form method '{method_id}' must declare at least one field"
        )));
    }
    let mut field_ids = std::collections::HashSet::new();
    for field in fields {
        validate_id(&field.id, "plugin form field id")?;
        if !field_ids.insert(field.id.clone()) {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' form method '{method_id}' contains duplicate field '{}'",
                field.id
            )));
        }
        if !matches!(
            field.field_type.as_str(),
            super::descriptor::FORM_FIELD_URL
                | super::descriptor::FORM_FIELD_SECRET
                | super::descriptor::FORM_FIELD_TEXT
        ) {
            return Err(Error::Config(format!(
                "plugin '{plugin_id}' form field '{}' uses unsupported type '{}'",
                field.id, field.field_type
            )));
        }
        validate_localized_text(
            &field.label,
            &format!(
                "plugin '{plugin_id}' form method '{method_id}' field '{}' label",
                field.id
            ),
        )?;
        if !field.placeholder.is_null() {
            validate_localized_text(
                &field.placeholder,
                &format!(
                    "plugin '{plugin_id}' form method '{method_id}' field '{}' placeholder",
                    field.id
                ),
            )?;
        }
    }
    Ok(())
}

fn icon_data_url(directory: &Path, relative: &str) -> Result<String> {
    let root = directory.canonicalize()?;
    let path = directory.join(relative).canonicalize()?;
    if !path.starts_with(&root) {
        return Err(Error::Config(format!(
            "plugin icon escapes its directory: {relative}"
        )));
    }
    if fs::metadata(&path)?.len() > MAX_ICON_BYTES {
        return Err(Error::Config(format!(
            "plugin icon exceeds {MAX_ICON_BYTES} bytes: {relative}"
        )));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => {
            return Err(Error::Config(format!(
                "unsupported plugin icon: {relative}"
            )))
        }
    };
    Ok(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(fs::read(path)?)
    ))
}

#[cfg(test)]
mod tests {
    use super::{validate_definition, PluginCatalog, PluginModuleDefinition};
    use crate::plugin::{
        definition::PluginDefinitionLoader,
        descriptor::{
            AddMethodDefinition, FormFieldDefinition, ProviderDefinition, ResourceDefinition,
        },
    };
    use std::path::PathBuf;

    #[test]
    fn repository_examples_have_valid_static_manifests() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/build-in");
        let sdk = tempfile::tempdir().unwrap();
        let catalog = PluginCatalog {
            roots: vec![root],
            definition_loader: PluginDefinitionLoader::for_test(sdk.path()).unwrap(),
            app_version: "1.0.1".into(),
        };
        let manifests: Vec<_> = catalog
            .manifests()
            .into_iter()
            .map(|(manifest, _)| manifest)
            .collect();
        let ids: Vec<_> = manifests
            .iter()
            .map(|manifest| manifest.id.as_str())
            .collect();
        assert!(ids.contains(&"dev.cursorbyok.examples.codex-auth"));
        assert!(ids.contains(&"dev.cursorbyok.examples.grok-auth"));
        assert!(ids.contains(&"dev.cursorbyok.plugins.antigravity-auth"));
        assert!(ids.contains(&"dev.cursorbyok.plugins.magpie"));
        let magpie = manifests
            .iter()
            .find(|manifest| manifest.id == "dev.cursorbyok.plugins.magpie")
            .unwrap();
        assert!(magpie.permissions.resource_origin);
        assert!(magpie.permissions.network.is_empty());
        for manifest in manifests
            .iter()
            .filter(|manifest| manifest.id != "dev.cursorbyok.plugins.magpie")
        {
            assert!(!manifest.permissions.resource_origin);
            assert!(!manifest.permissions.network.is_empty());
        }
    }

    fn provider(
        catalog: &str,
        has_models: bool,
        resource_type: Option<&str>,
    ) -> ProviderDefinition {
        ProviderDefinition {
            id: "gateway".into(),
            display_name: serde_json::json!("Gateway"),
            description: serde_json::Value::Null,
            provider_type: "openai".into(),
            resource_type: resource_type.map(str::to_owned),
            has_models,
            catalog: catalog.into(),
        }
    }

    fn resource(method: AddMethodDefinition) -> ResourceDefinition {
        ResourceDefinition {
            resource_type: "gateway".into(),
            display_name: serde_json::json!("Gateways"),
            add: vec![method],
            import: None,
            actions: Vec::new(),
            can_refresh: false,
            can_remove: false,
        }
    }

    #[test]
    fn accepts_form_add_methods_and_live_catalogs() {
        let definition = PluginModuleDefinition {
            providers: vec![provider("live", true, Some("gateway"))],
            resources: vec![resource(AddMethodDefinition {
                method_type: "form".into(),
                id: "connect".into(),
                display_name: serde_json::json!("Connect"),
                description: serde_json::Value::Null,
                callback: None,
                fields: vec![FormFieldDefinition {
                    id: "base-url".into(),
                    label: serde_json::json!("URL"),
                    field_type: "url".into(),
                    required: true,
                    placeholder: serde_json::json!("http://127.0.0.1:3425"),
                }],
            })],
        };
        validate_definition("dev.example", &definition).unwrap();
    }

    #[test]
    fn rejects_unsupported_form_fields_and_live_without_resources() {
        let mut definition = PluginModuleDefinition {
            providers: vec![provider("live", true, None)],
            resources: vec![resource(AddMethodDefinition {
                method_type: "form".into(),
                id: "connect".into(),
                display_name: serde_json::json!("Connect"),
                description: serde_json::Value::Null,
                callback: None,
                fields: vec![FormFieldDefinition {
                    id: "base-url".into(),
                    label: serde_json::json!("URL"),
                    field_type: "password".into(),
                    required: true,
                    placeholder: serde_json::Value::Null,
                }],
            })],
        };
        assert!(validate_definition("dev.example", &definition).is_err());
        definition.providers[0].resource_type = Some("gateway".into());
        definition.resources[0].add[0].fields[0].field_type = "url".into();
        validate_definition("dev.example", &definition).unwrap();
        definition.providers[0].catalog = "frozen".into();
        assert!(validate_definition("dev.example", &definition).is_err());
    }
}
