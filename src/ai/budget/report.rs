use super::{
    ContextSegment, OmissionPolicy, RequestPolicies, SelectionOmission, input_limit, input_tokens,
    text_tokens,
};
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct SegmentBudget {
    pub segment_id: usize,
    pub segment: ContextSegment,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub omission_reason: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetReport {
    pub model: String,
    pub window: usize,
    pub output_reserve: usize,
    pub safety_margin: usize,
    pub input_limit: Option<usize>,
    /// 降级时被占位化的早期工具结果条数（0=未触发；观测降级构成用）。
    #[serde(default)]
    pub pruned_tool_results: usize,
    pub input_tokens_before: usize,
    pub input_tokens_after: usize,
    pub tools_tokens: usize,
    /// Segment estimates count each serialized message independently plus its
    /// framing. They need not sum to the tokenizer's complete-payload estimate.
    pub counting: &'static str,
    pub outcome: &'static str,
    pub segments: Vec<SegmentBudget>,
    pub additional_segments: usize,
    pub preliminary_omissions: Vec<SelectionOmission>,
}

impl RequestPolicies {
    fn segment_counts(&self, model: &str, request: &Value) -> Vec<SegmentBudget> {
        request["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .zip(&self.messages)
            .map(|(message, policy)| SegmentBudget {
                segment_id: policy.report_id,
                segment: policy.segment.clone(),
                tokens_before: text_tokens(model, &message.to_string()) + 12,
                tokens_after: 0,
                omission_reason: None,
            })
            .collect()
    }

    /// Report actual serialized-payload selection, including rejected requests.
    /// Callers deliver this on the metadata observer channel, never as model text.
    pub(crate) fn fit_reported(
        &mut self,
        model: &str,
        request: &mut Value,
        window: usize,
        output: usize,
    ) -> (Result<usize>, BudgetReport) {
        let mut segments = self.segment_counts(model, request);
        let before = input_tokens(model, request);
        let result = self.fit(model, request, window, output);
        let after = self.segment_counts(model, request);
        let mut remaining: HashMap<_, _> =
            after.into_iter().map(|row| (row.segment_id, row)).collect();
        for segment in &mut segments {
            if let Some(after) = remaining.remove(&segment.segment_id) {
                segment.tokens_after = after.tokens_before;
            } else {
                segment.omission_reason = Some(
                    if segment.segment.omission == OmissionPolicy::OptionalBackground {
                        "optional_background"
                    } else {
                        "earlier_turn"
                    },
                );
            }
        }
        let mut inserted: Vec<_> = remaining.into_values().collect();
        inserted.sort_by_key(|row| row.segment_id);
        for mut segment in inserted {
            segment.tokens_after = segment.tokens_before;
            segment.tokens_before = 0;
            segments.push(segment);
        }
        let additional_segments = segments.len().saturating_sub(256);
        segments.truncate(256);
        let report = BudgetReport {
            model: model.into(),
            window,
            output_reserve: output,
            safety_margin: (window / 20).clamp(256, 4096),
            input_limit: input_limit(window, output).ok(),
            input_tokens_before: before,
            input_tokens_after: input_tokens(model, request),
            tools_tokens: request
                .get("tools")
                .map_or(0, |tools| text_tokens(model, &tools.to_string())),
            // R1 起计费口径：gpt 族用 tiktoken 精确分词；其余模型用内容
            // 启发式（CJK×2.0、ASCII÷3.4，含 1.12 安全系数，宁高不低）。
            counting: "tiered_estimate(tiktoken|content_heuristic_x1.12); segment_counts_are_not_additive",
            pruned_tool_results: self.pruned_tool_results,
            outcome: if result.is_ok() {
                "ready"
            } else {
                "local_rejection"
            },
            segments,
            additional_segments,
            preliminary_omissions: self.selection_omissions.clone(),
        };
        (result, report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::memory::{ContextMessage, SegmentOrigin, SegmentPurpose};
    use serde_json::json;

    #[test]
    fn report_counts_wire_payload_and_omissions_without_content() {
        let required = ContextMessage::new("system", "private evidence").with_segment(
            SegmentPurpose::SelectedEvidence,
            SegmentOrigin::KnowledgeSelection,
            Some("private-page-title"),
            OmissionPolicy::Required,
        );
        let background = ContextMessage::new("system", "private background".repeat(500))
            .with_segment(
                SegmentPurpose::CrossConversation,
                SegmentOrigin::StoredMessage,
                None,
                OmissionPolicy::OptionalBackground,
            );
        let context = vec![
            background,
            ContextMessage::new("user", "private question"),
            required,
        ];
        let mut policies = RequestPolicies::from_context(&context);
        let mut request = json!({"messages":context.iter().map(|m| json!({"role":m.role,"content":m.content})).collect::<Vec<_>>(),"tools":[{"name":"private_tool","description":"schema".repeat(40)}]});
        let (result, report) = policies.fit_reported("unknown", &mut request, 2048, 256);
        assert_eq!(result.unwrap(), report.input_tokens_after);
        assert_eq!(report.input_tokens_after, input_tokens("unknown", &request));
        assert!(report.tools_tokens > 0);
        assert_eq!(
            report.segments[0].omission_reason,
            Some("optional_background")
        );
        assert!(
            report
                .segments
                .iter()
                .any(|row| row.segment.purpose == SegmentPurpose::CurrentInput
                    && row.tokens_after > 0)
        );
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("private"));
        assert!(!request.to_string().contains("identity_hash"));
        assert!(!request.to_string().contains("omission_reason"));
    }

    #[test]
    fn rejected_required_context_has_an_honest_report() {
        let context = vec![ContextMessage::new("user", "private question".repeat(1000))];
        let mut policies = RequestPolicies::from_context(&context);
        let mut request = json!({"messages":[{"role":"user","content":context[0].content}]});
        let original = request.clone();
        let (result, report) = policies.fit_reported("unknown", &mut request, 2048, 256);
        assert!(result.is_err());
        assert_eq!(report.outcome, "local_rejection");
        assert_eq!(report.input_tokens_before, report.input_tokens_after);
        assert_eq!(request, original);
        assert!(
            report
                .segments
                .iter()
                .all(|row| row.omission_reason.is_none())
        );
    }
}
