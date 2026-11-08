//! Render only declared placeholders in the template. Values are appended as
//! opaque data and are never parsed again, even if they resemble placeholders.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Template {
    text: String,
    variables: BTreeSet<String>,
}

pub(crate) struct TextResources(BTreeMap<String, Template>);

impl TextResources {
    pub(crate) fn parse(body: &str) -> Result<Self> {
        Ok(Self(
            serde_json::from_str(body).context("prompt_resource_invalid")?,
        ))
    }

    pub(crate) fn literal(&self, name: &str) -> Result<&str> {
        let template = self.0.get(name).context("prompt_resource_missing")?;
        ensure!(
            template.variables.is_empty(),
            "prompt_variables_missing:{name}"
        );
        Ok(&template.text)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for (name, template) in &self.0 {
            let values = template
                .variables
                .iter()
                .map(|k| (k.as_str(), String::new()))
                .collect::<Vec<_>>();
            self.render(name, &values)?;
        }
        Ok(())
    }

    pub(crate) fn render(&self, name: &str, values: &[(&str, String)]) -> Result<String> {
        render_template(
            self.0.get(name).context("prompt_resource_missing")?,
            name,
            values,
        )
    }
}

pub(super) fn render(body: &str, name: &str, values: &[(&str, String)]) -> Result<String> {
    let resources: BTreeMap<String, Template> =
        serde_json::from_str(body).context("prompt_resource_invalid")?;
    let template = resources.get(name).context("prompt_resource_missing")?;
    render_template(template, name, values)
}

fn render_template(template: &Template, name: &str, values: &[(&str, String)]) -> Result<String> {
    let provided: BTreeMap<&str, &str> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
    ensure!(provided.len() == values.len(), "prompt_duplicate_variable");
    ensure!(
        template
            .variables
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == provided.keys().copied().collect(),
        "prompt_variables_mismatch:{name}"
    );
    let mut result = String::new();
    let mut remaining = template.text.as_str();
    let mut used = BTreeSet::new();
    while let Some(start) = remaining.find("<<") {
        result.push_str(&remaining[..start]);
        let after = &remaining[start + 2..];
        let end = after.find(">>").context("prompt_variable_unclosed")?;
        let key = &after[..end];
        result.push_str(provided.get(key).context("prompt_variable_unknown")?);
        used.insert(key);
        remaining = &after[end + 2..];
    }
    ensure!(
        used == provided.keys().copied().collect(),
        "prompt_variable_unused"
    );
    result.push_str(remaining);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_are_exact_and_user_data_is_never_reinterpreted() {
        let template = r#"{"goal":{"text":"Goal: <<goal>>; source: <<source>>","variables":["goal","source"]}}"#;
        assert_eq!(
            render(
                template,
                "goal",
                &[
                    ("goal", "原文 <<source>> {{secret:key}}".into()),
                    ("source", "topic/example".into())
                ]
            )
            .unwrap(),
            "Goal: 原文 <<source>> {{secret:key}}; source: topic/example"
        );
        assert!(render(template, "goal", &[("goal", "g".into())]).is_err());
        assert!(
            render(
                template,
                "goal",
                &[
                    ("goal", "g".into()),
                    ("source", "s".into()),
                    ("extra", "x".into())
                ]
            )
            .is_err()
        );
        assert!(render(template, "missing", &[]).is_err());
    }
}
