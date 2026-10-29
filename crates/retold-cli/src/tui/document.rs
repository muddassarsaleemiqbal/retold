//! Structured field projection and typed values for user-owned manifests.
use super::{form::Form, input::Input};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Field {
    pub path: String,
    pub value: Value,
    pub missing: bool,
}

pub fn fields(document: &Value) -> Vec<Field> {
    let mut fields = walk(&document["value"], "");
    fields.extend(
        document["missing"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|field| {
                Some(Field {
                    path: field["path"].as_str()?.into(),
                    value: field["value"].clone(),
                    missing: true,
                })
            }),
    );
    fields.retain(|field| !["/id", "/schema"].contains(&field.path.as_str()));
    fields.sort_by_key(|field| field.path != "/name");
    fields
}

fn walk(value: &Value, path: &str) -> Vec<Field> {
    match value {
        Value::Object(values) => values
            .iter()
            .flat_map(|(name, value)| {
                let escaped = name.replace('~', "~0").replace('/', "~1");
                walk(value, &format!("{path}/{escaped}"))
            })
            .collect(),
        Value::Array(values) => array_fields(values, path),
        _ => vec![Field {
            path: path.into(),
            value: value.clone(),
            missing: false,
        }],
    }
}

fn array_fields(values: &[Value], path: &str) -> Vec<Field> {
    let list = Field {
        path: path.into(),
        value: Value::Array(values.to_vec()),
        missing: false,
    };
    let children = values
        .iter()
        .enumerate()
        .flat_map(|(index, value)| walk(value, &format!("{path}/{index}")));
    std::iter::once(list).chain(children).collect()
}

#[derive(Debug, Clone)]
pub struct Editor {
    pub form: Form,
    pub original: Value,
    pub missing: bool,
    pub append_to: Option<Vec<Value>>,
    pub input: Input,
    pub problem: Option<String>,
}
impl Editor {
    pub fn type_key(&mut self, key: ratatui::crossterm::event::KeyEvent) {
        self.input.edit(key);
        self.problem = None;
    }

    pub fn new(form: Form, value: &Value) -> Self {
        let text = value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned);
        Self {
            form,
            original: value.clone(),
            missing: false,
            append_to: None,
            input: Input::with(&text),
            problem: None,
        }
    }
    pub fn reviewed(&self) -> Result<Form, String> {
        let textual = self.original.is_string();
        let replacement = if textual {
            Value::String(self.input.text().into())
        } else {
            serde_json::from_str(self.input.text()).map_err(|_| {
                "Use valid JSON for numbers, true/false, a list or an object.".to_owned()
            })?
        };
        let same_type = (replacement.is_boolean() && self.original.is_boolean())
            || (replacement.is_number() && self.original.is_number())
            || (replacement.is_string() && self.original.is_string())
            || (replacement.is_array() && self.original.is_array())
            || (replacement.is_object() && self.original.is_object());
        if !same_type {
            return Err("Keep the field's existing value type.".into());
        }
        let replacement = self.replacement(replacement);
        Ok(self.form.clone().with("value", &replacement.to_string()))
    }
}

impl Editor {
    fn replacement(&self, value: Value) -> Value {
        let Some(items) = &self.append_to else {
            return value;
        };
        let mut items = items.clone();
        items.push(value);
        Value::Array(items)
    }
}
pub fn list_context(document: &Value, path: &str) -> Option<(String, Vec<Value>, usize)> {
    let (parent, index) = path.rsplit_once('/')?;
    let index = index.parse::<usize>().ok()?;
    let items = document["value"].pointer(parent)?.as_array()?.clone();
    (index < items.len()).then(|| (parent.into(), items, index))
}
