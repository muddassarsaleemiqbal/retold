//! Every operation's options are described for graphical forms and rebuild valid CLI arguments.
use retold_cli::forms::{self, FieldKind, Value};

#[test]
fn every_operation_is_described_and_globals_are_left_to_the_client() {
    let operations = forms::operations();
    assert_eq!(operations.len(), retold_cli::operations().len());
    let globals = ["json", "workspace", "library", "quiet"];
    let leaked = operations
        .iter()
        .flat_map(|operation| &operation.fields)
        .any(|field| globals.contains(&field.id.as_str()));
    assert!(!leaked);
    if std::env::var_os("RETOLD_PRINT_FORMS").is_some() {
        for operation in &operations {
            let fields: Vec<_> = operation
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "{}:{:?}{}",
                        field.id,
                        field.kind,
                        if field.required { "*" } else { "" }
                    )
                })
                .collect();
            println!("{} | {}", operation.path, fields.join(", "));
        }
    }
}

fn limit_values(window: &forms::Field) -> Vec<(String, Value)> {
    let mut values = vec![
        ("scope".to_owned(), Value::Text("account".into())),
        ("dry-run".to_owned(), Value::On),
    ];
    if let FieldKind::Choice(choices) = &window.kind {
        values.push((window.id.clone(), Value::Text(choices[0].clone())));
    }
    values
}

#[test]
fn entered_values_become_the_arguments_the_cli_parses() {
    let operations = forms::operations();
    let limit = operations
        .iter()
        .find(|operation| operation.path == "account limit")
        .unwrap();
    assert!(limit.has_dry_run() && forms::arguments(limit, &[]).is_err());
    let window = limit
        .fields
        .iter()
        .find(|field| matches!(field.kind, FieldKind::Choice(_)))
        .unwrap();
    let built = forms::arguments(limit, &limit_values(window)).unwrap();
    assert_eq!(built[..2], ["account".to_owned(), "limit".to_owned()]);
    assert!(built.contains(&"--dry-run".to_owned()));
    let (_, text) = retold_cli::run_operation(&[built, vec!["--help".into()]].concat());
    assert!(text.contains("limit"));
}
