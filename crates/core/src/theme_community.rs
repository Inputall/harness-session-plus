//! Verified DreamSkin API. Packages are read in memory; CSS/scripts are never executed.
use futures_util::StreamExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    time::Duration,
};

const ORIGIN: &str = "https://api.dreamskin.cc";
const PACKAGE_LIMIT: usize = 32 * 1024 * 1024;
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}
async fn download(client: &reqwest::Client, path: &str, limit: usize) -> Result<Vec<u8>, String> {
    let response = client
        .get(format!("{ORIGIN}{path}"))
        .send()
        .await
        .map_err(|_| "无法连接 DreamSkin 社区，请稍后重试")?;
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return Err("社区返回失败或文件过大".into());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "下载中断")?;
        if bytes.len() + chunk.len() > limit {
            return Err("下载超过大小限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn version_id(id: &str) -> bool {
    id.starts_with("ver_")
        && id.len() <= 80
        && id.len() > 4
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn metadata(v: &Value) -> Result<(), String> {
    if !v["id"].as_str().is_some_and(version_id)
        || !v["name"]
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 200)
        || !v["packageBytes"]
            .as_u64()
            .is_some_and(|n| n > 0 && n <= PACKAGE_LIMIT as u64)
        || !v["packageSha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("社区主题元数据无效".into());
    }
    Ok(())
}
pub async fn page(offset: usize) -> Result<Value, String> {
    if offset > 100_000 {
        return Err("社区分页无效".into());
    }
    let client = client()?;
    let bytes = download(
        &client,
        &format!("/v1/themes?limit=48&offset={offset}&sort=recent"),
        1024 * 1024,
    )
    .await?;
    let mut page: Value = serde_json::from_slice(&bytes).map_err(|_| "社区清单无效")?;
    let items = page["items"].as_array_mut().ok_or("社区清单缺少条目")?;
    if items.len() > 48 {
        return Err("社区分页超过限制".into());
    }
    for item in items {
        let error = metadata(item).err();
        item["conversionReason"] =
            json!(error.unwrap_or_else(|| if item["applyCompatible"] == true {
                String::new()
            } else {
                "上游标记为旧格式，不能自动安装".into()
            }));
        // Do not return arbitrary preview URLs or untrusted metadata to the renderer.
        let keys = [
            "id",
            "name",
            "authorDisplayName",
            "license",
            "applyCompatible",
            "conversionReason",
        ];
        item.as_object_mut()
            .ok_or("社区条目无效")?
            .retain(|k, _| keys.contains(&k.as_str()));
    }
    Ok(page)
}
pub async fn install(id: &str) -> Result<Value, String> {
    if !version_id(id) {
        return Err("社区主题版本 ID 无效".into());
    }
    let client = client()?;
    let bytes = download(&client, &format!("/v1/themes/{id}"), 65536).await?;
    let meta: Value = serde_json::from_slice(&bytes).map_err(|_| "社区主题信息无效")?;
    metadata(&meta)?;
    if meta["id"] != id || meta["applyCompatible"] != true {
        return Err("该主题不能自动转换".into());
    }
    let package = download(
        &client,
        &format!("/v1/themes/{id}/download"),
        meta["packageBytes"].as_u64().unwrap() as usize,
    )
    .await?;
    convert_package(&package, &meta)
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && !path.contains(['\\', ':'])
        && !path.starts_with('/')
        && path.split('/').all(|c| !matches!(c, "" | "." | ".."))
}
fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if !safe_path(name) {
        return Err("主题包路径无效".into());
    }
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("主题包缺少 {name}"))?;
    if entry.size() == 0
        || entry.size() > limit as u64
        || entry.is_dir()
        || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
    {
        return Err("主题包条目大小或类型无效".into());
    }
    let mut data = Vec::new();
    (&mut entry)
        .take(limit as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "主题包解压失败")?;
    if data.len() > limit {
        return Err("解压数据超过限制".into());
    }
    Ok(data)
}
pub fn convert_package(package: &[u8], meta: &Value) -> Result<Value, String> {
    metadata(meta)?;
    if package.len() != meta["packageBytes"].as_u64().unwrap() as usize
        || !format!("{:x}", Sha256::digest(package))
            .eq_ignore_ascii_case(meta["packageSha256"].as_str().unwrap())
    {
        return Err("主题包大小或 SHA256 与社区审核信息不一致".into());
    }
    let mut archive =
        zip::ZipArchive::new(Cursor::new(package)).map_err(|_| "主题包不是有效 ZIP")?;
    if archive.len() > 100 {
        return Err("主题包条目过多".into());
    }
    let mut names = std::collections::HashSet::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|_| "主题包条目无效")?;
        if !safe_path(entry.name()) || !names.insert(entry.name().to_string()) {
            return Err("主题包包含异常路径或重复条目".into());
        }
    }
    let manifest: Value =
        serde_json::from_slice(&read_entry(&mut archive, "manifest.json", 65536)?)
            .map_err(|_| "主题包 manifest 无效")?;
    if manifest["packageVersion"] != 1
        || manifest["skinApiVersion"] != 1
        || manifest["themeId"] != meta["themeId"]
        || manifest["version"] != meta["version"]
    {
        return Err("主题包版本或身份不一致".into());
    }
    let files = manifest["files"].as_array().ok_or("主题包缺少文件清单")?;
    let config_bytes = read_entry(&mut archive, "theme.json", 65536)?;
    let config = std::str::from_utf8(&config_bytes).map_err(|_| "主题配置不是 UTF-8")?;
    let value: Value = serde_json::from_str(config).map_err(|_| "主题 JSON 无效")?;
    if value["id"] != manifest["themeId"] {
        return Err("主题配置与 manifest 身份不一致".into());
    }
    let image_path = value["image"].as_str().ok_or("主题配置缺少背景图片")?;
    let image = read_entry(&mut archive, image_path, crate::theme_import::IMAGE_LIMIT)?;
    for (name, bytes) in [
        ("theme.json", config_bytes.as_slice()),
        (image_path, image.as_slice()),
    ] {
        let f = files
            .iter()
            .find(|f| f["path"] == name)
            .ok_or("文件不在 manifest 中")?;
        if f["bytes"].as_u64() != Some(bytes.len() as u64)
            || f["sha256"].as_str() != Some(format!("{:x}", Sha256::digest(bytes)).as_str())
        {
            return Err("主题文件摘要与 manifest 不一致".into());
        }
    }
    let author = meta["authorDisplayName"].as_str().unwrap_or("");
    let license = meta["license"].as_str().unwrap_or("");
    if license.trim().is_empty() {
        return Err("主题缺少许可说明，不能自动安装".into());
    }
    let mut converted = crate::theme_import::convert(config, Some(&image), author, license)?;
    converted["id"] = json!(format!(
        "community-{}",
        meta["id"].as_str().unwrap().replace('_', "-")
    ));
    converted["name"] = meta["name"].clone();
    converted["communityVersion"] = meta["id"].clone();
    converted["source"] = json!("https://dreamskin.cc/gallery");
    converted["packageSha256"] = meta["packageSha256"].clone();
    converted["upstreamManifest"] = manifest; // Preserve license/provenance without executing package files.
    crate::theme_import::validate(&converted)?;
    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn fixture(path: &str) -> (Vec<u8>, Value) {
        let image = b"\x89PNG\r\n\x1a\nfixture";
        let config = serde_json::to_vec(&json!({"schemaVersion":1,"id":"test","name":"Test","image":path,"colors":{
            "background":"#ffffff","panel":"#ffffff","panelAlt":"#eeeeee","text":"#000000","muted":"#444444","accent":"#123456","line":"#aaaaaa"
        }})).unwrap();
        let file = |name: &str, data: &[u8]| json!({"path":name,"bytes":data.len(),"sha256":format!("{:x}",Sha256::digest(data))});
        let manifest = serde_json::to_vec(&json!({"packageVersion":1,"skinApiVersion":1,"themeId":"test","version":"1","files":[file("theme.json",&config),file(path,image)]})).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("manifest.json", manifest.as_slice()),
            ("theme.json", config.as_slice()),
            (path, image.as_slice()),
        ] {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let meta = json!({"id":"ver_test","name":"Test","themeId":"test","version":"1","authorDisplayName":"Author","license":"MIT","packageBytes":bytes.len(),"packageSha256":format!("{:x}",Sha256::digest(&bytes))});
        (bytes, meta)
    }
    #[test]
    fn packages_require_matching_identity_digest_and_safe_paths() {
        let (bytes, meta) = fixture("background.png");
        let skin = convert_package(&bytes, &meta).unwrap();
        assert_eq!(skin["id"], "community-ver-test");
        assert!(skin["wallpaper"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png"));
        let mut wrong = meta.clone();
        wrong["packageSha256"] = json!("0".repeat(64));
        assert!(convert_package(&bytes, &wrong).is_err());
        wrong = meta.clone();
        wrong["themeId"] = json!("other");
        assert!(convert_package(&bytes, &wrong).is_err());
        let (unsafe_bytes, unsafe_meta) = fixture("../background.png");
        assert!(convert_package(&unsafe_bytes, &unsafe_meta).is_err());
    }
    #[test]
    fn version_ids_cannot_change_download_origin_or_path() {
        for id in ["../etc", "ver_/download", "ver_x?url=evil", "https://evil"] {
            assert!(!version_id(id));
        }
        assert!(version_id("ver_1877a6b6015e7306f116"));
    }
}
