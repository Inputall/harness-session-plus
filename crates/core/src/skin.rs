//! Reversible appearance overlays. No native plugin or session API calls.
use crate::{
    cdp::{Client, Error, Result},
    discovery,
    management::{self, Options},
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
pub fn catalog() -> Value {
    let mut result: Value = serde_json::from_str(include_str!("../../../desktop/ui/skins.json"))
        .expect("bundled skin catalog");
    let sources: Value =
        serde_json::from_str(include_str!("../../../desktop/ui/themes/sources.json"))
            .expect("theme sources");
    for (index, (config, _)) in crate::theme_assets::ASSETS.iter().enumerate() {
        let source = &sources[index];
        let mut skin = crate::theme_import::convert(
            config,
            None,
            source["author"].as_str().unwrap(),
            source["license"].as_str().unwrap(),
        )
        .expect("verified bundled theme");
        skin["id"] = source["id"].clone();
        skin["name"] = source["name"].clone();
        skin["source"] = source["source_url"].clone();
        skin["description"] = source["description"].clone();
        skin["preview"] = json!(format!(
            "themes/{}/preview.jpg",
            source["id"].as_str().unwrap()
        ));
        skin["category"] = source["catalog"].clone();
        skin["tags"] = source["tags"].clone();
        result.as_array_mut().unwrap().push(skin);
    }
    result
}
pub fn select(id: &str) -> Option<Value> {
    let mut selected = catalog()
        .as_array()?
        .iter()
        .find(|s| s["id"] == id)?
        .clone();
    let sources: Value =
        serde_json::from_str(include_str!("../../../desktop/ui/themes/sources.json")).ok()?;
    if let Some(index) = sources.as_array()?.iter().position(|s| s["id"] == id) {
        selected["wallpaper"] =
            json!(crate::theme_import::wallpaper(crate::theme_assets::ASSETS[index].1).ok()?);
    }
    Some(selected)
}
pub async fn execute(options: &Options, action: &str, id: Option<&str>) -> Result<Value> {
    let skin = if action == "apply" {
        Some(select(id.ok_or(Error::InvalidResult)?).ok_or(Error::InvalidResult)?)
    } else {
        None
    };
    execute_with_skin(options, action, skin).await
}
pub async fn execute_with_skin(
    options: &Options,
    action: &str,
    skin: Option<Value>,
) -> Result<Value> {
    if !matches!(action, "status" | "apply" | "reset") {
        return Err(Error::InvalidResult);
    }
    if action == "apply" {
        crate::theme_import::validate(skin.as_ref().ok_or(Error::InvalidResult)?)
            .map_err(|_| Error::InvalidResult)?;
    }
    let (_, targets) = discovery::discover(options.port).await?;
    let target = discovery::select(&targets, options.target_id.as_deref())?;
    let client = Client::connect(
        target.websocket.as_deref().ok_or(Error::InvalidEndpoint)?,
        options.port,
    )
    .await?;
    let cancel = CancellationToken::new();
    let result = async {
        client.enable(&cancel).await?;
        let request = json!({"action":action,"skin":skin});
        management::evaluate_harness(
            &client,
            &format!("({})({request})", include_str!("../../../injected/skin.js")),
            &cancel,
        )
        .await
    }
    .await;
    client.close();
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_codex_themes_have_valid_palettes_and_verified_assets() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use sha2::{Digest, Sha256};
        let catalog = super::catalog();
        assert_eq!(catalog.as_array().unwrap().len(), 44);
        let sources: serde_json::Value =
            serde_json::from_str(include_str!("../../../desktop/ui/themes/sources.json")).unwrap();
        for s in &catalog.as_array().unwrap()[4..] {
            crate::theme_import::validate(s).unwrap();
            let source = sources
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == s["id"])
                .unwrap();
            let full = super::select(s["id"].as_str().unwrap()).unwrap();
            let bytes = STANDARD
                .decode(
                    full["wallpaper"]
                        .as_str()
                        .unwrap()
                        .split_once(',')
                        .unwrap()
                        .1,
                )
                .unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                source["image_sha256"].as_str().unwrap()
            );
            assert!(
                s["wallpaper"].is_null(),
                "catalog must not transfer full images"
            );
        }
    }
}
