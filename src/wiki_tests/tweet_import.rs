use super::*;
use crate::storage::{ContentPolicy, WikiPageDraft};
use std::path::{Path, PathBuf};
use crate::wiki::tweet_import::{FxTwitterResponse, parse_tweet_content};

#[test]
fn extract_tweet_id_parses_common_urls() {
    assert_eq!(
        extract_tweet_id("https://x.com/someone/status/123456789").as_deref(),
        Some("123456789")
    );
    assert_eq!(
        extract_tweet_id("https://twitter.com/someone/status/123456789?s=20").as_deref(),
        Some("123456789")
    );
    assert_eq!(
        extract_tweet_id("https://x.com/i/web/status/123456789/").as_deref(),
        Some("123456789")
    );
    assert_eq!(
        extract_tweet_id("https://mobile.twitter.com/someone/status/123456789#x").as_deref(),
        Some("123456789")
    );
    // 非推文链接 / 空输入 / 非数字 id
    assert_eq!(extract_tweet_id("https://example.com/not-a-tweet"), None);
    assert_eq!(extract_tweet_id(""), None);
    assert_eq!(extract_tweet_id("https://x.com/someone/status/abc"), None);
}

#[test]
fn file_url_roundtrips_local_paths() {
    // 普通路径原样往返
    let p = PathBuf::from("/home/pp/playground/ai/elsewhen");
    let url = path_to_file_url(&p);
    assert_eq!(url, "file:///home/pp/playground/ai/elsewhen");
    assert!(is_file_url(&url));
    assert_eq!(file_url_to_path(&url).as_deref(), Some(p.as_path()));
    // 空格与中文按字节编码后可还原
    let p2 = PathBuf::from("/tmp/我的 项目/a b");
    let url2 = path_to_file_url(&p2);
    assert!(is_file_url(&url2));
    assert!(!url2.contains(' '));
    assert_eq!(file_url_to_path(&url2).as_deref(), Some(p2.as_path()));
    // 非 file scheme 拒绝；远端 host 拒绝
    assert!(!is_file_url("https://example.com/x"));
    assert_eq!(file_url_to_path("https://example.com/x"), None);
    assert_eq!(file_url_to_path("file://other-host/tmp/x"), None);
    // localhost 宽容
    assert_eq!(
        file_url_to_path("file://localhost/tmp/x").as_deref(),
        Some(Path::new("/tmp/x"))
    );
}

#[test]
fn article_tweet_parses_blocks_instead_of_url_text() {
    // text 只是文章链接；长文在 article.content.blocks
    let json = r#"{
        "code": 200,
        "tweet": {
            "text": "https://x.com/i/article/2099410385648717824",
            "author": {"name": "伟大", "screen_name": "Huouo908070"},
            "article": {
                "title": "为什么你手握 Codex、Claude，依然赚不到钱？",
                "content": {
                    "blocks": [
                        {"text": "过去，一个人赚不到钱，往往还能找到很多具体的理由。"},
                        {"text": "这些理由过去确实成立，因为技术本身就是门槛。"}
                    ]
                }
            }
        }
    }"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    let out = parse_tweet_content(parsed).unwrap();
    assert_eq!(
        out.title.as_deref(),
        Some("为什么你手握 Codex、Claude，依然赚不到钱？")
    );
    assert_eq!(
        out.text,
        "过去，一个人赚不到钱，往往还能找到很多具体的理由。\n\n这些理由过去确实成立，因为技术本身就是门槛。"
    );
    assert_eq!(out.author_name.as_deref(), Some("伟大"));
    assert_eq!(out.screen_name.as_deref(), Some("Huouo908070"));
}

#[test]
fn normal_tweet_parses_text_without_article() {
    let json = r#"{
        "code": 200,
        "tweet": {
            "text": "just setting up my twttr",
            "author": {"name": "jack", "screen_name": "jack"}
        }
    }"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    let out = parse_tweet_content(parsed).unwrap();
    assert_eq!(out.text, "just setting up my twttr");
    assert_eq!(out.title, None);
    assert!(out.image_urls.is_empty());
}

#[test]
fn article_tweet_collects_images_at_atomic_blocks() {
    // 文章型推文配图：atomic block 经 entityRanges.key（1-based）关联
    // media_entities，纯解析层在原位插入占位标记并收集直链。
    let json = r#"{
        "code": 200,
        "tweet": {
            "text": "https://x.com/i/article/2108373211285266465",
            "author": {"name": "冬青", "screen_name": "DongQingAsk"},
            "article": {
                "title": "小红书虚拟产品卖什么？",
                "media_entities": [
                    {"media_info": {"original_img_url": "https://pbs.twimg.com/media/AAA.jpg"}},
                    {"media_info": {"original_img_url": "https://pbs.twimg.com/media/BBB.png"}}
                ],
                "content": {
                    "blocks": [
                        {"type": "unstyled", "text": "第一段"},
                        {"type": "atomic", "text": " ", "entityRanges": [{"key": 1, "offset": 0, "length": 1}]},
                        {"type": "unstyled", "text": "第二段"},
                        {"type": "atomic", "text": " ", "entityRanges": [{"key": 2, "offset": 0, "length": 1}]},
                        {"type": "unstyled", "text": "结尾"}
                    ]
                }
            }
        }
    }"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    let out = parse_tweet_content(parsed).unwrap();
    // 图片原位插入占位，正文顺序不变
    assert_eq!(
        out.text,
        "第一段\n\n{{tweet-image-0}}\n\n第二段\n\n{{tweet-image-1}}\n\n结尾"
    );
    // 直链按出现顺序收集
    assert_eq!(
        out.image_urls,
        vec![
            "https://pbs.twimg.com/media/AAA.jpg".to_string(),
            "https://pbs.twimg.com/media/BBB.png".to_string(),
        ]
    );
    assert_eq!(out.title.as_deref(), Some("小红书虚拟产品卖什么？"));
}

#[test]
fn article_atomic_block_with_missing_media_is_skipped() {
    // atomic block 指向的 media_entities 缺失/无 URL → 跳过，不产占位也不崩。
    let json = r#"{
        "code": 200,
        "tweet": {
            "article": {
                "title": "t",
                "media_entities": [],
                "content": {"blocks": [
                    {"type": "unstyled", "text": "甲"},
                    {"type": "atomic", "text": " ", "entityRanges": [{"key": 5}]},
                    {"type": "unstyled", "text": "乙"}
                ]}
            }
        }
    }"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    let out = parse_tweet_content(parsed).unwrap();
    assert_eq!(out.text, "甲\n\n乙");
    assert!(out.image_urls.is_empty());
}

#[test]
fn normal_tweet_appends_top_level_photos() {
    // 普通推文（非文章型）：media.photos 追加在正文末尾。
    let json = r#"{
        "code": 200,
        "tweet": {
            "text": "看看这个",
            "author": {"name": "u", "screen_name": "u"},
            "media": {"photos": [
                {"url": "https://pbs.twimg.com/media/X1.jpg", "type": "photo"},
                {"url": "https://pbs.twimg.com/media/X2.png", "type": "photo"}
            ]}
        }
    }"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    let out = parse_tweet_content(parsed).unwrap();
    assert_eq!(
        out.text,
        "看看这个\n\n{{tweet-image-0}}\n\n{{tweet-image-1}}"
    );
    assert_eq!(out.image_urls.len(), 2);
    assert_eq!(out.image_urls[0], "https://pbs.twimg.com/media/X1.jpg");
}

#[test]
fn article_without_blocks_falls_back_to_text() {
    let json = r#"{
        "code": 200,
        "tweet": {
            "text": "https://x.com/i/article/1",
            "article": {"title": "只有标题没正文"}
        }
    }"#;
    let parsed: FxTwitterResponse = serde_json::from_str(json).unwrap();
    let out = parse_tweet_content(parsed).unwrap();
    assert_eq!(out.text, "https://x.com/i/article/1");
    assert_eq!(out.title.as_deref(), Some("只有标题没正文"));
}
