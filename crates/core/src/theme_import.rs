//! Convert Codex Dream Skin data; never execute package CSS or JavaScript.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub const IMAGE_LIMIT: usize = 5 * 1024 * 1024;
fn field<'a>(v: &'a Value, key: &str, limit: usize) -> Result<&'a str, String> {
    let s = v[key].as_str().ok_or_else(|| format!("缺少字段 {key}"))?;
    if s.is_empty() || s.len() > limit || s.chars().any(char::is_control) {
        return Err(format!("字段 {key} 无效"));
    }
    Ok(s)
}
pub fn color(s: &str) -> Result<String, String> {
    if s.starts_with('#')
        && matches!(s.len(), 4 | 7 | 9)
        && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(if s.len() == 4 {
            format!(
                "#{}",
                s[1..].chars().flat_map(|c| [c, c]).collect::<String>()
            )
        } else {
            s.to_lowercase()
        });
    }
    if let Some(body) = s.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let n: Vec<f64> = body
            .split(',')
            .map(|s| s.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| "颜色格式不支持")?;
        if n.len() == 4
            && n.iter().all(|v| v.is_finite())
            && n[..3]
                .iter()
                .all(|v| *v >= 0. && *v <= 255. && v.fract() == 0.)
            && n[3] >= 0.
            && n[3] <= 1.
        {
            return Ok(format!("rgba({},{},{},{})", n[0], n[1], n[2], n[3]));
        }
    }
    Err("只支持十六进制颜色或 rgba 数值，不接受 CSS 表达式".into())
}
pub fn wallpaper(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() || bytes.len() > IMAGE_LIMIT {
        return Err("图片大小必须在 1 字节至 5 MiB 之间".into());
    }
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err("图片必须为 PNG、JPEG 或 WebP，SVG 和脚本不支持".into());
    };
    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}
pub fn convert(
    config: &str,
    image: Option<&[u8]>,
    author: &str,
    license: &str,
) -> Result<Value, String> {
    if config.len() > 65536 || author.len() > 200 || license.len() > 4000 {
        return Err("主题配置或来源信息过大".into());
    }
    let v: Value = serde_json::from_str(config).map_err(|_| "主题 JSON 无效")?;
    if v["schemaVersion"] != 1 {
        return Err("仅支持 schemaVersion 1 的 theme.json".into());
    }
    let name = field(&v, "name", 200)?;
    let c = &v["colors"];
    let get = |key| field(c, key, 100).and_then(color);
    let background = get("background")?;
    if !background.starts_with('#') || background.len() != 7 {
        return Err("主题背景需要不透明十六进制颜色".into());
    }
    let mut digest = Sha256::new();
    digest.update(config.as_bytes());
    if let Some(b) = image {
        digest.update(b);
    }
    digest.update(author.as_bytes());
    digest.update(license.as_bytes());
    let id = format!("imported-{:x}", digest.finalize());
    Ok(
        json!({"id":id,"name":name,"description":v["tagline"].as_str().unwrap_or("通用主题转换"),"bg":background,"surface":get("panel")?,"hover":get("panelAlt")?,"text":get("text")?,"muted":get("muted")?,"accent":get("accent")?,"border":get("line")?,"wallpaper":image.map(wallpaper).transpose()?,"author":author,"license":license,"converted":true,"limitations":"保留配色和壁纸；不执行主题包中的 CSS/脚本，不还原专属布局、装饰与插件效果。"}),
    )
}
pub fn validate(s: &Value) -> Result<(), String> {
    let id = field(s, "id", 100)?;
    if !id
        .bytes()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("主题 ID 无效".into());
    }
    field(s, "name", 200)?;
    for key in [
        "bg", "surface", "hover", "text", "muted", "accent", "border",
    ] {
        color(field(s, key, 100)?)?;
    }
    if let Some(url) = s["wallpaper"].as_str() {
        let (prefix, encoded) = url.split_once(',').ok_or("壁纸无效")?;
        if ![
            "data:image/png;base64",
            "data:image/jpeg;base64",
            "data:image/webp;base64",
        ]
        .contains(&prefix)
            || encoded.len() > IMAGE_LIMIT * 4 / 3 + 8
        {
            return Err("壁纸无效".into());
        }
        let bytes = STANDARD.decode(encoded).map_err(|_| "壁纸编码无效")?;
        if wallpaper(&bytes)? != url {
            return Err("壁纸类型与数据不匹配".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_executable_colors_and_invalid_images() {
        for c in [
            "url(https://x)",
            "#fff;display:none",
            "rgba(0,0,0,NaN)",
            "rgba(256,0,0,1)",
        ] {
            assert!(color(c).is_err());
        }
        assert!(wallpaper(b"<svg></svg>").is_err());
        assert_eq!(
            color("rgba(255, 238, 218, 0.78)").unwrap(),
            "rgba(255,238,218,0.78)"
        );
    }
    #[test]
    fn converts_actual_codex_configuration() {
        let v = convert(
            include_str!("../../../assets/themes/caishen-lite/theme.json"),
            None,
            "ChannelerH",
            "MIT",
        )
        .unwrap();
        validate(&v).unwrap();
        assert_eq!(v["accent"], "#d65f3b");
        assert_eq!(v["surface"], "rgba(255,251,244,0.9)");
        assert!(v["id"].as_str().unwrap().starts_with("imported-"));
    }
}
