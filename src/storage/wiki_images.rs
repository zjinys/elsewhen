//!wiki 图片引用的占位符管理。
//!
//! 存储不变式：`content_md` 里任何 tweet 配图都只以占位符
//! `![]({{tweet-image:tweet_images/<tweet_id>/img_N.<ext>}})` 形态存在——
//! 占位符内是**相对数据目录的确定性路径**，永不出现本机绝对路径。
//! 本机路径只活在两处：落盘文件本身，和渲染前动态解析的瞬间。
//!
//! - 写侧（`fold_tweet_image_refs`）：任何写入 `wiki_pages.content_md` 的
//!   正文都先折叠——把（导入时替换占位符产生的）本机绝对路径或既有占位符
//!   归一化为占位符。幂等。
//! - 读侧（`resolve_wiki_image_refs`）：渲染/出站前解析——占位符 → 本机
//!   绝对路径；文件缺失则原样保留（退化为可见的死文本，不伪装）。
//!   文件由快照 apply 直接落盘（§31b 起字节不落库，无 DB 自愈）。
//! - 同步：占位符就是 op/快照里的形态；字节经快照 v3 的文件段从发送端
//!   磁盘直达接收端磁盘，DB 里的 `wiki_assets` 只留文件名片做台账。
//!
//! hand-rolled 扫描（无 regex 依赖）：只认 `![…](…)` 图片括号组形态，
//! 不碰其它 markdown 语法。

use std::path::Path;

/// rel 的目录前缀（相对 data_dir）。
pub(crate) const TWEET_IMAGE_DIR: &str = "tweet_images";

/// 从落盘全路径提取 rel（相对 data_dir，正斜杠统一）；不在 data_dir 下返回 None。
pub(crate) fn tweet_image_rel(path: &Path, data_dir: &Path) -> Option<String> {
    let rel = path.strip_prefix(data_dir).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// rel 形态校验：`tweet_images/<纯数字>/img_<纯数字>.<字母数字扩展名>`。
/// 快照 apply 与本地折叠共用这一个事实标准。
pub(crate) fn rel_is_valid(rel: &str) -> bool {
    let rest = match rel.strip_prefix(&format!("{TWEET_IMAGE_DIR}/")) {
        Some(r) => r,
        None => return false,
    };
    let (dir, file) = match rest.split_once('/') {
        Some(x) => x,
        None => return false,
    };
    dir.bytes().all(|b| b.is_ascii_digit())
        && file
            .strip_prefix("img_")
            .and_then(|f| f.split_once('.'))
            .is_some_and(|(n, ext)| {
                !n.is_empty()
                    && n.bytes().all(|b| b.is_ascii_digit())
                    && mime_for_ext(ext).is_some()
            })
}

/// 扩展名 → MIME（白名单；未知扩展名不收资产）。
pub(crate) fn mime_for_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

fn placeholder(rel: &str) -> String {
    format!("![]({{{{tweet-image:{rel}}}}})")
}

/// 从图片括号组内容提取 rel：
/// - 占位符 `{{tweet-image:REL}}` → REL；
/// - 本机绝对路径 `…/tweet_images/<id>/img_N.ext` → `tweet_images/…` 尾部
///   （路径不可移植，但尾部是确定性构成，存量页/跨机页靠它归一化）；
/// - 其它（http:// 链接、用户手写路径等）→ None，原样保留。
fn rel_from_inner(inner: &str) -> Option<String> {
    if let Some(rest) = inner.strip_prefix("{{tweet-image:") {
        let rel = rest.strip_suffix("}}")?;
        return rel_is_valid(rel).then(|| rel.to_string());
    }
    let i = inner.rfind(&format!("/{TWEET_IMAGE_DIR}/"))?;
    let rel = &inner[i + 1..];
    rel_is_valid(rel).then(|| rel.to_string())
}

/// 扫描 `![…](…)`：内部可归一化为 tweet 图片的，统一改写为占位符。
/// 幂等：占位符再次折叠结果不变。
pub(crate) fn fold_tweet_image_refs(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i < markdown.len() {
        // 图片起始：`![`（前面不能再有 `!`/`[` 语义，这里按字面认）
        if bytes[i] == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(paren) = markdown[i..].find("](") {
                let alt = &markdown[i + 2..i + paren];
                let inner_start = i + paren + 2;
                if let Some(close) = markdown[inner_start..].find(')') {
                    let inner = &markdown[inner_start..inner_start + close];
                    if let Some(rel) = rel_from_inner(inner) {
                        // 求到 rel 才改写；alt 与占位符形态无关，一律空 alt。
                        out.push_str(&placeholder(&rel));
                        i = inner_start + close + 1;
                        continue;
                    }
                    let _ = alt;
                }
            }
        }
        // 逐字节推进以正确处理 UTF-8 边界
        let ch_len = markdown[i..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        out.push_str(&markdown[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// 解析占位符/可归一化的绝对路径为本机绝对路径。
/// - 文件已存在 → 直接用；
/// - 缺文件且 `wiki_assets` 有行 → 落盘自愈后再用；
/// - 都没有 → 原样保留（可见死文本）。
/// 任何失败都不阻断显示：该函数除 IO 错误外不返回 Err，
/// 单张图自愈失败保持占位符不动。
pub(crate) fn resolve_wiki_image_refs(store: &super::Store, markdown: &str) -> String {
    let data_dir = match store.path.parent() {
        Some(p) => p.to_path_buf(),
        None => return markdown.to_string(),
    };
    let mut out = String::with_capacity(markdown.len());
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i < markdown.len() {
        if bytes[i] == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(paren) = markdown[i..].find("](") {
                let inner_start = i + paren + 2;
                if let Some(close) = markdown[inner_start..].find(')') {
                    let inner = &markdown[inner_start..inner_start + close];
                    if let Some(rel) = rel_from_inner(inner) {
                        let target = data_dir.join(&rel);
                        if target.exists() {
                            out.push_str(&format!("![]({})", target.display()));
                        } else {
                            // 文件缺失（本机未导入/快照没带到）：占位符原样保留，
                            // 退化为可见死文本（§31b 起字节不落库，无本机自愈）。
                            out.push_str(&format!("![]({{{{tweet-image:{rel}}}}})"));
                        }
                        i = inner_start + close + 1;
                        continue;
                    }
                    // 非 tweet 图片（http 等）原样放过
                }
            }
        }
        let ch_len = markdown[i..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        out.push_str(&markdown[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// 从正文中提取全部 tweet 图片 rel（占位符/绝对路径两种形态）。
pub(crate) fn collect_tweet_image_rels(markdown: &str) -> Vec<String> {
    let mut rels = Vec::new();
    let bytes = markdown.as_bytes();
    let mut i = 0;
    while i < markdown.len() {
        if bytes[i] == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(paren) = markdown[i..].find("](") {
                let inner_start = i + paren + 2;
                if let Some(close) = markdown[inner_start..].find(')') {
                    let inner = &markdown[inner_start..inner_start + close];
                    if let Some(rel) = rel_from_inner(inner) {
                        rels.push(rel);
                        i = inner_start + close + 1;
                        continue;
                    }
                }
            }
        }
        let ch_len = markdown[i..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);
        i += ch_len;
    }
    rels
}

#[cfg(test)]
mod tests {
    use super::*;

    const REL: &str = "tweet_images/2099471972589576451/img_1.jpg";

    #[test]
    fn rel_validation_accepts_only_deterministic_tweet_image_paths() {
        assert!(rel_is_valid(REL));
        assert!(rel_is_valid("tweet_images/1/img_1.webp"));
        assert!(!rel_is_valid("tweet_images/abc/img_1.jpg")); // 非数字 tweet id
        assert!(!rel_is_valid("tweet_images/123/img_x.jpg")); // 序号非数字
        assert!(!rel_is_valid("tweet_images/123/nope_1.jpg")); // 文件名前缀
        assert!(!rel_is_valid("tweet_images/123/img_1.exe")); // 扩展名未收
        assert!(!rel_is_valid("elsewhere/123/img_1.jpg")); // 目录前缀
        assert!(!rel_is_valid("tweet_images/123/img_1.")); // 空扩展名
    }

    #[test]
    fn fold_rewrites_absolute_paths_and_is_idempotent() {
        let md = format!(
            "正文 A。\n\n![](\\home\\pp\\.local\\share\\elsewhen\\{REL})\n\n正文 B。\n\n![]({REL})",
            REL = format!("/home/pp/.local/share/elsewhen/{REL}"),
        );
        let folded = fold_tweet_image_refs(&md);
        assert!(folded.contains(&format!("![]({{{{tweet-image:{REL}}}}})")));
        assert!(folded.contains("正文 A。") && folded.contains("正文 B。"));
        // 幂等：再折一遍不变
        assert_eq!(fold_tweet_image_refs(&folded), folded);
    }

    #[test]
    fn fold_leaves_other_image_and_link_forms_untouched() {
        let md = "![](https://example.com/a.jpg) 和 [链接](https://x.com) 和 ![](/etc/passwd) 和 ![]({{tweet-image:坏 格式}})";
        let folded = fold_tweet_image_refs(md);
        assert_eq!(folded, md);
    }
}
