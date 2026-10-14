//! Explicit network-only acceptance; does not apply themes or touch session data.
#[tokio::main]
async fn main() -> Result<(), String> {
    let page = hsp_core::theme_community::page(0).await?;
    let id = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["conversionReason"] == "")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let theme = hsp_core::theme_community::install(id).await?;
    println!(
        "{}",
        serde_json::json!({"total":page["total"],"pageSize":page["items"].as_array().unwrap().len(),"installed":theme["name"],"version":theme["communityVersion"],"wallpaperBytes":theme["wallpaper"].as_str().unwrap().len(),"license":theme["license"],"digest":theme["packageSha256"]})
    );
    Ok(())
}
