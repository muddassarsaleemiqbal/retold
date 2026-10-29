use retold_deliver::{Aspect, Chapter, Post, chapters_text, validate_chapters, validate_posts};
#[test]
fn portable_metadata_checks_unicode_bytes_and_final_hashtag_text() {
    let mut p = Post {
        platform: "youtube".into(),
        title: "मेरी कहानी".into(),
        description: "A real story".into(),
        hashtags: vec!["#story".into()],
    };
    assert!(validate_posts(&[p.clone()]).is_ok());
    p.description = "庭".repeat(1700);
    assert!(validate_posts(&[p.clone()]).is_err());
    p.description = "x".repeat(2200);
    assert!(validate_posts(&[p.clone()]).is_err());
    p.description = "good".into();
    p.title = "bad<title>".into();
    assert!(validate_posts(&[p]).is_err());
}
#[test]
fn chapter_order_and_aspects_are_validated() {
    assert_eq!(Aspect::parse("9:16").unwrap().dimensions(), [1080, 1920]);
    assert!(Aspect::parse("../../oops").is_err());
    let c = vec![
        Chapter {
            seconds: 0.0,
            title: "Opening".into(),
        },
        Chapter {
            seconds: 61.0,
            title: "Turn".into(),
        },
    ];
    assert!(validate_chapters(&c, 90.0).is_ok());
    assert_eq!(
        chapters_text(&c).unwrap(),
        "00:00:00 Opening\n00:01:01 Turn\n"
    );
    assert!(validate_chapters(&c, 60.0).is_err());
    assert!(validate_chapters(&[c[1].clone(), c[0].clone()], 90.0).is_err());
}
