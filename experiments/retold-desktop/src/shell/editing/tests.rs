use super::compared_text;

#[test]
fn conflict_comparison_matches_section_name_after_reordering() {
    let sections = [
        ("ending", "The saved ending."),
        ("opening", "The saved opening."),
    ];
    assert_eq!(
        compared_text(sections, Some("opening")),
        "The saved opening."
    );
    assert!(compared_text(sections, Some("removed")).contains("no matching section"));
}
