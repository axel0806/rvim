// SPDX-License-Identifier: Apache-2.0
//! The `|` command separator next to `:s`, `:g` and `:normal`, checked
//! against real Vim. A `|` inside the pattern or the replacement of `:s`
//! (or in the pattern of a range address) is text; after the replacement's
//! delimiter and the flags it separates commands again. `:g`, `:v` and
//! `:normal` take the rest of the line. In the keys, `@` is the delimiter
//! where the expression has `#` or `/` in it.

mod common;
use common::{check, cross};

#[test]
fn bar_in_substitute_pattern_is_text() {
    check(&[
        ("aab\nxyz\nbba", r#":%s/\v(a|b)+/X/g<CR>"#),
        ("aab\nxyz\nbba", r#":%s/\v(a|b)/X/g<CR>"#),
        ("aab\nxyz\nbba", r#":%s/\v(a|b)+/X/<CR>"#),
        ("ab cb", r#":s/\va|b/X/g<CR>"#),
        ("a|b\nxyz", r#":%s/a|b/X/<CR>"#),
        ("a|b ab", r#":s/a|b/X/g<CR>"#),
        ("a|b\na", r#":%s/\va\|b/X/g<CR>"#),
        ("a|b\nab", r#":%s/a\|b/X/g<CR>"#),
        ("aab", r#":s/\v(a|b)+/X/|s/X/Y/<CR>"#),
        ("aab", r#":s/\v(a|b)+/X/ | s/X/Y/<CR>"#),
        ("a|b", r#":s/a|b<CR>"#),
        ("ab", r#":s/a<CR>"#),
    ]);
}

#[test]
fn bar_in_substitute_replacement_is_text() {
    check(&[
        ("ab", r#":s/a/x|y/<CR>"#),
        ("ab", r#":s/a/x|y/|s/b/z/<CR>"#),
        ("ab", r#":s/a/x\|y/<CR>"#),
        ("aXb", r#":s/X/|/|s/|/-/<CR>"#),
        ("aXb", r#":s/X/|/ | s/|/-/<CR>"#),
        ("ab\nab", r#":%s/a/b|c/<CR>"#),
        ("abcb", r#":s/b/|/g<CR>"#),
        ("ab", r#":s/a/b|c<CR>"#),
    ]);
}

#[test]
fn bar_with_custom_delimiter() {
    check(&[
        ("a|b", r#":s#a|b#X#<CR>"#),
        ("a|b ab", r#":s#a|b#X#g<CR>"#),
        ("a|b", r#":s+a|b+X+<CR>"#),
        ("a|b", r#":s+a|b+X|Y+<CR>"#),
        ("a|b", r#":s#a|b#X#|s#X#Z#<CR>"#),
        ("a|b", r#":s#a#|#|s#|#-#<CR>"#),
        ("a|b", r#":s+a|b+X+|s+X+Z+<CR>"#),
    ]);
}

#[test]
fn bar_after_substitute_separates_commands() {
    check(&[
        ("abc", r#":s/a/b/|s/c/d/<CR>"#),
        ("abc", r#":s/a/b/ | s/c/d/<CR>"#),
        ("abc abc", r#":s/a/b/g|%s/b/c/g<CR>"#),
        ("abc\nabc", r#":%s/a/b/g|%s/b/c/g<CR>"#),
        ("abc\nabc", r#":%s/a/b/g | %s/b/c/g<CR>"#),
        ("abc", r#":s/a/b/g|s/z/y/<CR>x"#),
        ("abc", r#":s/z/y/|s/a/b/<CR>"#),
        ("abc", r#":s/a/b/ | normal Ax<CR>"#),
        ("abc", r#":s/a/b/|normal Ax|y<CR>"#),
        ("x", r#":s@x@\=0 || 2@|s/2/Z/<CR>"#),
        ("x", r#":s@x@\=1 ? 'y' : 'n'@|s/y/Q/<CR>"#),
        ("x", r#":s@x@\=printf('%d|%d', 1, 2)@|s/|/-/<CR>"#),
    ]);
}

#[test]
fn bar_in_global_and_vglobal() {
    check(&[
        ("a|b\nx\na|b", r#":g/a|b/s/|/-/<CR>"#),
        ("aab\nxyz\nbba", r#":g/\va|b/s//X/g<CR>"#),
        ("aab\nxyz\nbba", r#":g/b/s//X/<CR>"#),
        ("aab\nxyz\nbba", r#":v/\va|b/s/x/Q/<CR>"#),
        ("ab\ncd\nab", r#":g/a/s/a/x/|s/b/y/<CR>"#),
        ("aab\nxyz\nbba", r#":g/\va|b/s//X/g|s/X/Y/<CR>"#),
        ("a\nb\na", r#":g/a/s/a/x|y/<CR>"#),
        ("a|b\nq", r#":g/q/s/q/a|b/<CR>"#),
        ("a\nb\nc", r#":g/b/s/b/x/|s/x/y|z/<CR>"#),
        ("a\nb\nc", r#":v/b/normal Ax|y<CR>"#),
        ("a\nb\nc", r#":g/a|b/d<CR>"#),
        ("a|b\nb\nc", r#":g/a|b/d<CR>"#),
    ]);
}

#[test]
fn bar_in_range_address_is_text() {
    check(&[
        ("a|b\nxyz\nq", r#":/a|b/d<CR>"#),
        ("a|b\nxyz\nq", r#":/a|b/s/x/Q/<CR>"#),
        ("xyz\na|b\nq", r#":/\va|b/d<CR>"#),
        ("xyz\na|b\nq", r#":/\va|b/s/b/Q/<CR>"#),
        ("q\na|b\nq", r#":?a|b?d<CR>"#),
        ("q\nab\nq\nab", r#":/a\|b/d<CR>"#),
        ("q\nx\nq", r#":2|d<CR>"#),
        ("q\nx\nq", r#":2|s/x/y/<CR>"#),
    ]);
}

#[test]
fn bar_chains_over_several_texts() {
    let texts = ["aab\nxyz\nbba", "a|b\nab", "x\n|\ny"];
    let keys = [
        r#":%s/\v(a|b)+/X/g|%s/X/Y/g<CR>"#,
        r#":%s/a|b/Q/g<CR>"#,
        r#":%s/|/-/g|%s/-/+/g<CR>"#,
        r#":g/\v(a|b)/s//Q/g|s/Q/R/<CR>"#,
    ];
    check(&cross(&texts, &keys));
}
