// SPDX-License-Identifier: GPL-3.0-only

pub mod cdp;
pub mod cli;
pub mod compatibility;
pub mod config;
pub mod cursor;
pub mod fs_util;
pub mod inspector;
pub mod language_pack;
pub mod process;
pub mod renderer;
pub mod translation;
pub mod update;

pub const PRODUCT_NAME: &str = "Cursor 中文启动器";
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const TRANSLATION_SCHEMA_VERSION: u32 = 1;

pub fn embedded_translation_catalog() -> anyhow::Result<translation::TranslationCatalog> {
    let mut catalog: translation::TranslationCatalog =
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/translations/zh-CN.json"
        )))?;
    for addition in [
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/translations/settings.zh-CN.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/translations/automation.zh-CN.json"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/translations/workbench.zh-CN.json"
        )),
    ] {
        let addition: translation::TranslationCatalog = serde_json::from_str(addition)?;
        if catalog.schema_version != addition.schema_version
            || catalog.locale != addition.locale
            || catalog.glossary_version != addition.glossary_version
        {
            anyhow::bail!("embedded translation catalogs use incompatible metadata");
        }
        catalog.nls.extend(addition.nls);
        catalog.runtime.extend(addition.runtime);
        catalog.extensions.extend(addition.extensions);
    }
    Ok(catalog)
}

pub fn embedded_compatibility_catalog() -> anyhow::Result<compatibility::CompatibilityCatalog> {
    let catalog = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/compatibility.json"
    )))?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_assets_pass_release_validation() {
        let translations = embedded_translation_catalog().unwrap();
        assert!(translations.validate(true).is_empty());
        assert!(translations.runtime.iter().any(|entry| {
            entry.id == "renderer.agents-menu.new-terminal"
                && entry.scopes == ["[data-component='menu-item-label']"]
        }));
        assert!(translations.runtime.iter().any(|entry| {
            entry.id == "renderer.ide-welcome.open-project"
                && entry.scopes == [".empty-screen-button"]
        }));

        let compatibility = embedded_compatibility_catalog().unwrap();
        compatibility.validate().unwrap();
        assert!(
            compatibility
                .find_by_commit("6b2afae0257df2bb5e1835f15165dc2f0de056b0")
                .is_some()
        );
    }
}
