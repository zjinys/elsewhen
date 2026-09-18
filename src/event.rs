use chrono::{DateTime, Utc};

pub struct NewEvent<'a> {
    pub raw_text: &'a str,
    pub occurred_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub source: &'static str,
}

impl<'a> NewEvent<'a> {
    pub fn now(raw_text: &'a str) -> Self {
        let now = Utc::now();
        Self {
            raw_text,
            occurred_at: now,
            recorded_at: now,
            source: "capture",
        }
    }
}

pub struct EventSummary {
    pub recorded_at: String,
    pub raw_text: String,
}

/// 从一条文本里解析出的「规范标注」实体：@人名 = 人物，#事情 = 事情/项目。
/// 用户用标注把实体写死，AI 提取时以标注为权威，不再靠猜。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnnotationSet {
    pub people: Vec<String>,
    pub targets: Vec<String>,
}

impl AnnotationSet {
    /// 合并另一组标注（people/targets 各按出现顺序去重）
    pub fn merge(&mut self, other: &AnnotationSet) {
        for p in &other.people {
            if !self.people.contains(p) {
                self.people.push(p.clone());
            }
        }
        for t in &other.targets {
            if !self.targets.contains(t) {
                self.targets.push(t.clone());
            }
        }
    }
}

/// 解析规范标注：`@`（或全角 `＠`）后跟人物名，`#`（或全角 `＃`）后跟事情/项目名。
/// 名字：紧随符号的连续字符（中文/字母/数字/括号——括号可作同名备注，如 `@张伟（市场部）`），
/// 遇空白或标点（，。、！？；：,.!?;:/\|）或另一个标注符号即结束；自动去重。
pub fn parse_annotations(text: &str) -> AnnotationSet {
    let mut out = AnnotationSet::default();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        let kind = if c == '@' || c == '＠' {
            Some(true) // true = 人物
        } else if c == '#' || c == '＃' {
            Some(false) // false = 事情/项目
        } else {
            None
        };
        let Some(is_person) = kind else {
            i += 1;
            continue;
        };
        let start = i + 1;
        let mut end = start;
        while end < chars.len() {
            let ch = chars[end];
            let stops = ch.is_whitespace()
                || matches!(
                    ch,
                    '，' | '。'
                        | '、'
                        | '！'
                        | '？'
                        | '；'
                        | '：'
                        | ','
                        | '.'
                        | '!'
                        | '?'
                        | ';'
                        | ':'
                        | '/'
                        | '\\'
                        | '|'
                        | '@'
                        | '＠'
                        | '#'
                        | '＃'
                );
            if stops {
                break;
            }
            end += 1;
        }
        let name: String = chars[start..end].iter().collect();
        if !name.is_empty() {
            if is_person {
                if !out.people.contains(&name) {
                    out.people.push(name);
                }
            } else if !out.targets.contains(&name) {
                out.targets.push(name);
            }
        }
        i = end;
    }
    out
}

/// 批量合并多条文本（事件列表）里的标注
pub fn parse_annotations_many<'a>(texts: impl Iterator<Item = &'a str>) -> AnnotationSet {
    let mut out = AnnotationSet::default();
    for t in texts {
        let set = parse_annotations(t);
        out.merge(&set);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_person_and_target() {
        let a = parse_annotations("今天 @张伟 负责的 #双链路付款 有进展");
        assert_eq!(a.people, vec!["张伟"]);
        assert_eq!(a.targets, vec!["双链路付款"]);
    }

    #[test]
    fn parens_are_part_of_name_for_disambiguation() {
        let a = parse_annotations("跟 @张伟（市场部） 对齐，@张伟（设计） 也来了");
        assert_eq!(a.people, vec!["张伟（市场部）", "张伟（设计）"]);
    }

    #[test]
    fn stops_at_punctuation_and_whitespace() {
        let a = parse_annotations("@李婷，负责#贵州交付；@老王 走了");
        assert_eq!(a.people, vec!["李婷", "老王"]);
        assert_eq!(a.targets, vec!["贵州交付"]);
    }

    #[test]
    fn fullwidth_symbols_and_dedup() {
        let a = parse_annotations("＠张伟 ＃双链路 #双链路 @张伟");
        assert_eq!(a.people, vec!["张伟"]);
        assert_eq!(a.targets, vec!["双链路"]);
    }

    #[test]
    fn empty_and_lone_symbols_ignored() {
        let a = parse_annotations("今天 @ 和 # 都空，@ 结尾也忽略");
        assert!(a.people.is_empty());
        assert!(a.targets.is_empty());
    }
}
