//! Inspect the actual Bevy module, which the engine-free harness does not compile.
fn test_module_is_last(source: &str) -> bool {
    let file = syn::parse_file(source).expect("valid Rust module source");
    file.items.iter().enumerate().all(|(index, item)| {
        !matches!(item, syn::Item::Mod(module) if module.ident == "tests")
            || index + 1 == file.items.len()
    })
}

#[test]
fn production_geometry_item_order_keeps_reexports_before_tests() {
    let source = include_str!("../../benilla-app/src/net/p2p/world/native_geometry/mod.rs");
    assert!(test_module_is_last(source));
    // Exercise the original failure against the complete production file, not a
    // parallel module list which cannot see newly appended app-only items.
    let appended = format!("{source}\n#[cfg(test)] use core::mem as regression;\n");
    assert!(!test_module_is_last(&appended));
}

#[test]
fn item_order_guard_uses_rust_items_instead_of_comment_or_brace_matching() {
    assert!(test_module_is_last(
        r#"use core::mem; #[cfg(test)] mod tests {
            // mod tests {} and use statements here are not top-level items.
            fn literal() { let _ = "} use core::mem;"; }
        }"#
    ));
    assert!(!test_module_is_last(
        "#[cfg(test)] mod tests {} #[cfg(test)] pub use core::mem;"
    ));
}
