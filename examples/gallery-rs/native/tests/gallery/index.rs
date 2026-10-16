//! The gallery on a headless engine: its screens push and render.

#[test]
fn navigation_pushes_its_screens() {
    let (engine, rx, mut screen) = crate::common::open("Navigation");
    for (label, title) in [
        ("Large title", "Large title"),
        ("Custom header items and colors", "Custom"),
        ("Unsaved changes (prevent_back)", "Unsaved changes"),
    ] {
        crate::common::press(&engine, &rx, &mut screen, label);
        assert!(
            screen.titles.iter().any(|t| t.1 == title),
            "{label}: {:?}",
            screen.titles
        );
    }
}
