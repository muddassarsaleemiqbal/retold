//! Missing declared properties are offered only when declared by the resource schema.
use serde_json::{Value, json};
const DEPTH_LIMIT: usize = 16;
pub(super) fn missing(kind: &str, value: &Value) -> Vec<Value> {
    let root = if kind == "guides" {
        retold_engine::retold_guide::guide_schema()
    } else {
        retold_engine::retold_guide::channel_schema()
    };
    Schema { root: &root }.collect(value, (&root, "", 0))
}
struct Schema<'a> {
    root: &'a Value,
}
impl Schema<'_> {
    fn collect(&self, value: &Value, position: (&Value, &str, usize)) -> Vec<Value> {
        let (schema, path, depth) = position;
        if depth >= DEPTH_LIMIT {
            return Vec::new();
        }
        let schema = self.resolve(schema);
        let Some(properties) = schema["properties"].as_object() else {
            return Vec::new();
        };
        properties
            .iter()
            .flat_map(|(key, property)| {
                let pointer = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                let property = self.resolve(property);
                let existing = value.get(key);
                if let Some(existing) = existing {
                    return self.collect(existing, (property, &pointer, depth + 1));
                }
                self.sample(property, depth)
                    .map(|sample| json!({"path":pointer,"value":sample}))
                    .into_iter()
                    .collect()
            })
            .collect()
    }
    fn resolve<'a>(&'a self, schema: &'a Value) -> &'a Value {
        let referenced = self.reference(schema);
        if let Some(referenced) = referenced {
            return referenced;
        }
        let branch = ["anyOf", "oneOf"].iter().find_map(|union| {
            schema[*union]
                .as_array()?
                .iter()
                .find(|branch| branch["type"] != "null")
        });
        branch.map_or(schema, |branch| self.reference(branch).unwrap_or(branch))
    }
    fn reference<'a>(&'a self, schema: &Value) -> Option<&'a Value> {
        let path = schema["$ref"].as_str()?.strip_prefix('#')?;
        self.root.pointer(path)
    }
}
impl Schema<'_> {
    /// A missing object section is offered with its required members filled, so it validates.
    fn sample(&self, schema: &Value, depth: usize) -> Option<Value> {
        let value = primitive(schema)?;
        let declared = schema.get("default").is_some_and(|value| !value.is_null());
        if declared || !value.is_object() || depth >= DEPTH_LIMIT {
            return Some(value);
        }
        let required = schema["required"].as_array().into_iter().flatten();
        let members = required.filter_map(|key| {
            let key = key.as_str()?;
            let property = self.resolve(&schema["properties"][key]);
            Some((key.to_owned(), self.sample(property, depth + 1)?))
        });
        Some(Value::Object(members.collect()))
    }
}
fn primitive(schema: &Value) -> Option<Value> {
    let default = schema.get("default").filter(|value| !value.is_null());
    if let Some(value) = default {
        return Some(value.clone());
    }
    let enumeration = schema["enum"].as_array().and_then(|values| values.first());
    if let Some(value) = enumeration {
        return Some(value.clone());
    }
    let kind = schema["type"].as_str().or_else(|| {
        schema["type"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find(|kind| *kind != "null")
    })?;
    let primitives = [
        ("string", json!("")),
        ("boolean", json!(false)),
        ("array", json!([])),
        ("object", json!({})),
    ];
    primitives
        .into_iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, value)| value)
        .or_else(|| ["number", "integer"].contains(&kind).then(|| json!(0)))
}
