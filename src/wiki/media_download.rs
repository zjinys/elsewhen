//! 推文媒体图片下载（fxtwitter + DoH 解析），自 tweet_import.rs 分段平移。

use anyhow::{Context, Result};

use super::{FxTweetContent, image_placeholder};
/// 把文章配图下载到 `{data_dir}/tweet_images/{tweet_id}/img_N.{ext}`，
/// 并把正文里的临时占位标记 `{{tweet-image-N}}` 替换为存储态占位符
/// `![]({{tweet-image:tweet_images/…}})`（相对路径为主体，显示时才解析为本机路径）。
/// 单张失败仅留说明文字、不影响其它图与正文。
///
/// 网络策略：先直连；被墙的图床（如 pbs.twimg.com）直连失败的图，
/// 若用户配置了 DoH（`should_use_doh`），改走 DoH+ECH 隧道重试。
pub(crate) fn download_tweet_images(
    tweet_id: &str,
    data_dir: &std::path::Path,
    content: &mut FxTweetContent,
) {
    let dir = data_dir.join("tweet_images").join(tweet_id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        // 目录都建不了：所有占位换为说明，放弃下载。
        for (idx, _) in content.image_urls.iter().enumerate() {
            content.text = content.text.replace(
                &image_placeholder(idx),
                &format!("*[图片目录创建失败：{e}]*"),
            );
        }
        return;
    }
    let client = match crate::http_proxy::with_outbound_proxy(
        reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(30)),
    )
    .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    // 第一遍：直连下载；记录失败的图待 DoH 兜底。成功图不写本机绝对路径——
    // 正文里落占位符（相对 data_dir 的确定性路径），显示时动态解析；
    // 字节由保存路径收进 wiki_assets 随快照同步。
    let mut direct_failed: Vec<usize> = Vec::new();
    let mut replacements: Vec<Option<String>> = Vec::with_capacity(content.image_urls.len());
    for (idx, url) in content.image_urls.iter().enumerate() {
        match download_one_image(&client, url, &dir, idx) {
            Ok(path) => match crate::storage::wiki_images::tweet_image_rel(&path, data_dir) {
                Some(rel) => replacements.push(Some(format!("![]({{{{tweet-image:{rel}}}}})"))),
                None => replacements.push(Some(format!("![]({})", path.display()))),
            },
            Err(_) => {
                direct_failed.push(idx);
                replacements.push(None);
            }
        }
    }
    // 第二遍：直连失败的图，若配置了 DoH 则走 DoH+ECH 隧道重试。
    // 用户显式挂代理（proxy_mode=custom）时 DoH 让位：「挂了代理就全走代理」。
    if !direct_failed.is_empty() {
        if let Some(net) = load_doh_settings() {
            if net.should_use_doh() && net.proxy_mode != "custom" {
                for &idx in &direct_failed {
                    let url = &content.image_urls[idx];
                    match download_one_image_via_doh(url, &net.doh_uri, &dir, idx) {
                        Ok(path) => {
                            replacements[idx] = Some(
                                match crate::storage::wiki_images::tweet_image_rel(&path, data_dir)
                                {
                                    Some(rel) => format!("![]({{{{tweet-image:{rel}}}}})"),
                                    None => format!("![]({})", path.display()),
                                },
                            );
                        }
                        Err(_) => {
                            // DoH 也失败：保持 None，下面统一写失败说明。
                        }
                    }
                }
            }
        }
    }
    // 统一替换占位：成功的写本地路径，最终仍失败的写说明。
    for (idx, rep) in replacements.iter().enumerate() {
        let placeholder = image_placeholder(idx);
        let text = match rep {
            Some(md) => md.clone(),
            None => "*[配图下载失败：直连不可用，且未配置可用的 DoH 隧道]*".to_string(),
        };
        content.text = content.text.replace(&placeholder, &text);
    }
}

/// 读取 DoH 配置（仅在需要 DoH 兜底时才开库，正常直连成功时零开销）。
/// 读不到（如库不可用）返回 None，下载退化为直连-only。
fn load_doh_settings() -> Option<crate::storage::NetworkSettings> {
    let config = crate::config::AppConfig::load().ok()?;
    let store = crate::storage::Store::open(&config.database_path).ok()?;
    store.network_settings().ok()
}

/// 经 DoH+ECH 隧道下载单张图片到本地（被墙图床的兜底通道）。
fn download_one_image_via_doh(
    url: &str,
    doh_uri: &str,
    dir: &std::path::Path,
    idx: usize,
) -> Result<std::path::PathBuf> {
    let bytes =
        crate::net_doh::https_get_bytes_via_doh(url, doh_uri, std::time::Duration::from_secs(30))
            .with_context(|| format!("DoH 下载失败 {url}"))?;
    write_image_bytes(&bytes, url, dir, idx)
}

/// 直连下载单张图片到本地，返回落盘路径。
fn download_one_image(
    client: &reqwest::blocking::Client,
    url: &str,
    dir: &std::path::Path,
    idx: usize,
) -> Result<std::path::PathBuf> {
    let bytes = client
        .get(url)
        .header(
            "User-Agent",
            "elsewhen/0.1 (local-first personal knowledge base)",
        )
        .send()
        .with_context(|| format!("请求失败 {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP 错误 {url}"))?
        .bytes()
        .context("读取图片字节失败")?;
    write_image_bytes(&bytes, url, dir, idx)
}

/// 把图片字节写入 `img_{idx+1}.{ext}`，扩展名从 URL 推断（默认 jpg），返回落盘路径。
fn write_image_bytes(
    bytes: &[u8],
    url: &str,
    dir: &std::path::Path,
    idx: usize,
) -> Result<std::path::PathBuf> {
    let ext = url
        .split('?')
        .next()
        .and_then(|p| p.rsplit('.').next())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| matches!(e.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp"))
        .unwrap_or_else(|| "jpg".to_string());
    let path = dir.join(format!("img_{}.{ext}", idx + 1));
    std::fs::write(&path, bytes).with_context(|| format!("写入图片失败 {}", path.display()))?;
    Ok(path)
}
