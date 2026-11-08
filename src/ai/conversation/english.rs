//! Conservative English conversation signals. These never grant write approval.

fn prose(text: &str) -> String {
    let mut fenced = false;
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with("    ") || line.starts_with('\t') {
            continue;
        }
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || line.starts_with('>') {
            continue;
        }
        let mut quoted = None;
        let mut previous = ' ';
        for ch in line.chars() {
            if let Some(end) = quoted {
                if ch == end {
                    quoted = None;
                    out.push(' ');
                }
                continue;
            }
            quoted = match ch {
                '"' | '`' => Some(ch),
                '“' => Some('”'),
                '「' => Some('」'),
                '『' => Some('』'),
                '\'' if !previous.is_alphanumeric() => Some('\''),
                _ => None,
            };
            if quoted.is_none() {
                out.push(if ch == '’' { '\'' } else { ch });
            }
            previous = ch;
        }
        out.push('\n');
    }
    out.to_lowercase()
}

fn guarded(clause: &str) -> bool {
    let words = clause
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();
    words.iter().any(|w| {
        matches!(
            *w,
            "not"
                | "nothing"
                | "never"
                | "don't"
                | "didn't"
                | "doesn't"
                | "haven't"
                | "hasn't"
                | "can't"
                | "cannot"
                | "won't"
                | "wouldn't"
                | "without"
                | "if"
                | "unless"
                | "example"
                | "example:"
                | "whether"
                | "failed"
                | "failure"
        )
    }) || clause.starts_with("how ")
        || clause.starts_with("what does ")
        || clause.starts_with("the phrase ")
        || clause.starts_with("the sentence ")
}

fn clauses(text: &str) -> Vec<String> {
    prose(text)
        .split(['.', '!', '?', ';', '\n', '。', '！', '？', '；'])
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) fn completed_write(text: &str) -> bool {
    clauses(text).iter().any(|clause| {
        !guarded(clause)
            && (matches!(
                clause.as_str(),
                "saved" | "created" | "updated" | "archived" | "generated"
            ) || [
                "i saved",
                "i've saved",
                "i have saved",
                "i created",
                "i've created",
                "i have created",
                "i updated",
                "i've updated",
                "i have updated",
                "i archived",
                "i've archived",
                "i generated",
                "i've generated",
                "i have generated",
                "i added",
                "i've added",
                "has been saved",
                "have been saved",
                "was saved",
                "saved successfully",
                "has been created",
                "was created",
                "created successfully",
                "has been updated",
                "added to the knowledge base",
                "saved to the knowledge base",
            ]
            .iter()
            .any(|p| clause.contains(p)))
    })
}

pub(super) fn future_promise(text: &str) -> bool {
    let clean = prose(text);
    if [
        "here is the draft",
        "here's the draft",
        "here are the results",
        "the result is",
        "draft below",
        "results below",
        "please review the draft",
    ]
    .iter()
    .any(|p| clean.contains(p))
    {
        return false;
    }
    clauses(text).iter().any(|clause| {
        !guarded(clause)
            && [
                "please wait",
                "give me a moment",
                "one moment",
                "i'll get back to you",
                "i will get back to you",
                "i'll do that",
                "i will do that",
                "i'll take care of",
                "i will take care of",
                "i'll prepare",
                "i will prepare",
                "i'm going to prepare",
                "i'll look into",
                "i will look into",
                "i'll check",
                "i will check",
                "i'll search",
                "i will search",
                "i'll save",
                "i will save",
                "let me check",
                "let me search",
                "let me prepare",
                "let me save",
                "let me look into",
                "i'll work on",
                "i will work on",
                "i'll send",
                "i will send",
            ]
            .iter()
            .any(|p| clause.contains(p))
    })
}

fn requested_action(text: &str, verbs: &[&str]) -> bool {
    let clean = prose(text);
    if clean.trim_end().ends_with('?') {
        return false;
    }
    clauses(text).iter().any(|clause| {
        if guarded(clause) {
            return false;
        }
        let mut action = clause.trim();
        for prefix in [
            "please ",
            "could you ",
            "can you ",
            "would you ",
            "will you ",
            "i want you to ",
            "i'd like you to ",
            "help me ",
            "go ahead and ",
        ] {
            if let Some(rest) = action.strip_prefix(prefix) {
                action = rest.trim();
            }
        }
        verbs.iter().any(|verb| {
            action
                .strip_prefix(verb)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        })
    })
}

pub(crate) fn write_request(text: &str) -> bool {
    requested_action(
        text,
        &[
            "save", "create", "update", "archive", "record", "remember", "submit", "add", "edit",
            "revise", "replace", "remove", "delete",
        ],
    )
}

pub(crate) fn page_revision_request(text: &str) -> bool {
    requested_action(
        text,
        &[
            "edit", "update", "revise", "rewrite", "correct", "replace", "remove", "delete",
        ],
    )
}

pub(super) fn progress_question(text: &str) -> bool {
    matches!(
        text.trim()
            .trim_end_matches(['?', '!', '.', '？', '。', '！'])
            .trim()
            .to_lowercase()
            .as_str(),
        "continue"
            | "please continue"
            | "go on"
            | "carry on"
            | "any update"
            | "any updates"
            | "what's the status"
            | "what is the status"
            | "what's the progress"
            | "any progress"
            | "how is it going"
            | "how's it going"
            | "is it done"
            | "are you done"
            | "done yet"
            | "what happened"
            | "what's next"
            | "what about it"
            | "where are we"
    )
}

pub(super) fn empty_failure(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    text.chars().count() <= 250
        && [
            "the model returned no content",
            "the model returned an empty",
            "the model response was empty",
            "could not generate a reply",
            "couldn't generate a reply",
            "failed to generate a reply",
        ]
        .iter()
        .any(|p| text.contains(p))
}

pub(super) fn incomplete_reply(text: &str) -> bool {
    let text = text.to_lowercase();
    [
        "task remains unfinished",
        "this turn is unfinished",
        "have not delivered a usable result",
        "no usable result has been delivered",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claims_distinguish_completed_work_from_negations_examples_and_quotes() {
        for text in [
            "Saved \"Notes\".",
            "Created \"Notes\".",
            "I've saved \"Notes\". Would you like more detail?",
            "I created a page. The next step is not ready.",
            "The page has been updated.",
            "Sorry, I have generated the derivative.",
        ] {
            assert!(completed_write(text), "{text}");
        }
        for text in [
            "I have not saved it.",
            "I haven't saved it.",
            "The page was never created.",
            "If saved successfully, it will appear here.",
            "Example: I created a page.",
            "He said \"I saved the page\".",
            "> I have saved the page.",
            "```text\nI have saved the page.\n```",
            "I will save the page.",
            "The draft is awaiting review.",
        ] {
            assert!(!completed_write(text), "{text}");
        }
    }
    #[test]
    fn write_requests_do_not_promote_questions_negations_or_quoted_code() {
        for text in [
            "Please save this as a topic.",
            "Create a page from this source.",
            "Please update this page.",
            "Could you archive this page.",
            "Remember this preference.",
        ] {
            assert!(write_request(text), "{text}");
        }
        for text in [
            "Don't save this.",
            "Please do not update this page.",
            "Save nothing; explain it instead.",
            "Did you save the page?",
            "Can you create a page?",
            "How do I save this page.",
            "If you save it, what happens.",
            "The record says create a page.",
            "\"Save this page\"",
            "```\nSave this page.\n```",
            "Yesterday I created a page.",
        ] {
            assert!(!write_request(text), "{text}");
        }
    }
    #[test]
    fn future_promises_preserve_delivered_drafts_and_ignore_examples() {
        for text in [
            "Please wait. I'll prepare the draft.",
            "Let me check the sources.",
            "I'll get back to you shortly.",
        ] {
            assert!(future_promise(text), "{text}");
        }
        for text in [
            "I will not save it.",
            "Example: I'll prepare the draft.",
            "Here is the draft:\n- Fact one\nPlease review the draft.",
            "> Let me check the sources.",
            "```\nPlease wait\n```",
        ] {
            assert!(!future_promise(text), "{text}");
        }
        for text in [
            "Continue",
            "Any updates?",
            "What's the status?",
            "Done yet?",
        ] {
            assert!(progress_question(text), "{text}");
        }
    }
}

#[cfg(test)]
mod acknowledgement_tests {
    #[test]
    fn english_acknowledgements_are_whole_utterances_and_never_approval() {
        use super::super::content_chat::{is_confirmation, is_declination};
        for text in ["Yes", "Okay!", "Sure", "Agreed", "Go ahead"] {
            assert!(is_confirmation(text), "{text}");
            assert!(!is_declination(text), "{text}");
        }
        for text in [
            "No",
            "Nope",
            "Cancel",
            "Not now",
            "Never mind",
            "Don't save",
        ] {
            assert!(is_declination(text), "{text}");
            assert!(!is_confirmation(text), "{text}");
        }
        for text in [
            "notes",
            "noted",
            "notice",
            "nothing",
            "Yes, don't save it",
            "Sure, do not update it",
        ] {
            assert!(!is_confirmation(text), "{text}");
            assert!(!is_declination(text), "{text}");
        }
    }
}
