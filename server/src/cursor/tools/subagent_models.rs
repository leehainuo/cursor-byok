//! Resolves Task model selections to configured BYOK model identities.

use crate::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ByokModel {
    pub hash: String,
    pub model_id: String,
    pub display_name: String,
}

pub(crate) fn resolve<'a>(models: &'a [ByokModel], requested: &'a str) -> Result<&'a str> {
    if let Some(model) = models.iter().find(|model| model.hash == requested) {
        return Ok(&model.hash);
    }
    let key = display_key(requested);
    let mut matches = models
        .iter()
        .filter(|model| model.model_id == requested || display_key(&model.display_name) == key)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        let key = display_key(without_suffix(requested));
        matches = models
            .iter()
            .filter(|model| display_key(without_suffix(&model.display_name)) == key)
            .collect();
    }
    match matches.as_slice() {
        [] => Ok(requested),
        [model] => Ok(&model.hash),
        _ => Err(Error::Protocol(format!(
            "Task model {requested:?} is ambiguous; use a configured model hash: {}",
            matches
                .iter()
                .map(|model| model.hash.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn display_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn without_suffix(name: &str) -> &str {
    let name = name.trim_end();
    match name.strip_suffix(')').and_then(|rest| rest.rfind('(')) {
        Some(start) => name[..start].trim_end(),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(hash: &str, model_id: &str, display_name: &str) -> ByokModel {
        ByokModel {
            hash: hash.into(),
            model_id: model_id.into(),
            display_name: display_name.into(),
        }
    }

    #[test]
    fn hashes_take_precedence_over_other_models_aliases() {
        let models = [
            model("hash-a", "hash-b", "Model A"),
            model("hash-b", "other-id", "Model B"),
        ];
        assert_eq!(resolve(&models, "hash-b").unwrap(), "hash-b");
    }

    #[test]
    fn duplicate_ids_or_display_names_require_a_hash() {
        for models in [
            [
                model("a", "gpt-x", "Model A"),
                model("b", "gpt-x", "Model B"),
            ],
            [
                model("a", "id-a", "My Model"),
                model("b", "id-b", "my-model"),
            ],
            [
                model("a", "id-a", "My Model (Provider A)"),
                model("b", "id-b", "My Model (Provider B)"),
            ],
        ] {
            let requested = if models[0].model_id == "gpt-x" {
                "gpt-x"
            } else {
                "my-model"
            };
            let error = resolve(&models, requested).unwrap_err().to_string();
            assert!(error.contains("ambiguous"));
            assert!(error.contains("a, b"));
            assert_eq!(resolve(&models, "b").unwrap(), "b");
        }
    }

    #[test]
    fn full_display_names_take_precedence_over_suffix_aliases() {
        let models = [
            model("a", "id-a", "My Model (Provider A)"),
            model("b", "id-b", "My Model (Provider B)"),
        ];
        assert_eq!(resolve(&models, "MY-MODEL (PROVIDER B)").unwrap(), "b");
    }
}
