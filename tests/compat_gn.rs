// SPDX-License-Identifier: Apache-2.0
//! `gn` and `gN` in Normal, Visual and operator-pending mode, checked against
//! real Vim, plus the error paths of `:r` (the harness cannot create files,
//! so successful `:r` is tested by unit tests in `src/ex.rs`).
//!
//! Every case sets the search pattern itself: Vim keeps the last pattern
//! between cases, rvim does not.

mod common;
use common::check;

const TEXT: &str = "foo bar foo baz foo";

#[test]
fn gn_selects_match_under_or_after_cursor() {
    check(&[
        (TEXT, "/foo<CR>0gnd"),
        (TEXT, "/foo<CR>0gn"),
        (TEXT, "/foo<CR>0gn<Esc>x"),
        (TEXT, "/foo<CR>0llgnd"),
        (TEXT, "/foo<CR>0wgnd"),
        (TEXT, "/foo<CR>0wwgnd"),
        (TEXT, "/foo<CR>$gnd"),
        (TEXT, "/foo<CR>0gnc X<Esc>"),
        (TEXT, "/foo<CR>0ygnP"),
        (TEXT, "/foo<CR>0gny$p"),
        (TEXT, "/foo<CR>02gnd"),
        (TEXT, "/foo<CR>0d2gn"),
        (TEXT, "/foo<CR>0gngnd"),
        (TEXT, "/foo<CR>0vgnd"),
        (TEXT, "/foo<CR>0vlgnd"),
        (TEXT, "/foo<CR>$vgnd"),
        (TEXT, "/foo<CR>$vbgnd"),
        (TEXT, "/foo<CR>0gn~"),
        (TEXT, "/foo<CR>0gnU"),
        (TEXT, "/foo<CR>0gnu"),
        (TEXT, "/foo<CR>0gnJ"),
        (TEXT, "/foo<CR>0gnyiw"),
        ("foo bar foo\nabcdefghij", "/foo<CR>$0gn<Esc>jx"),
        ("foo bar foo\nabcdefghij", "/foo<CR>$gN<Esc>jx"),
        ("foo bar\nbaz foo", "/foo<CR>$gnd"),
        ("foo bar\nbaz foo", "/foo<CR>G$gnd"),
        ("foo bar\nbaz foo", "/foo<CR>0jgnd"),
        ("foo\nbar", "/foo<CR>0gnd"),
        ("foo\nbar", "/foo<CR>:set nows<CR>0gnd"),
        ("foo\nbar", "/foo<CR>:set nows<CR>Ggnx"),
        ("xfoo foofoo", "/foo<CR>0gnd"),
        ("afoo bfoo", "/foo<CR>0llgnd"),
        ("aaaa", "/aa<CR>0gnd"),
        ("aaaa", "/aa<CR>$gnd"),
        ("ab", "/x*<CR>0gnd"),
    ]);
}

#[test]
fn big_gn_selects_match_before_cursor() {
    check(&[
        (TEXT, "/foo<CR>gNd"),
        (TEXT, "/foo<CR>0gNd"),
        (TEXT, "/foo<CR>0wgNd"),
        (TEXT, "/foo<CR>0wgN<Esc>x"),
        (TEXT, "/foo<CR>$gNd"),
        (TEXT, "/foo<CR>$2gNd"),
        (TEXT, "/foo<CR>0gNcX<Esc>"),
        (TEXT, "/foo<CR>0gNy$p"),
        (TEXT, "/foo<CR>$vgNd"),
        (TEXT, "/foo<CR>$vgNgNd"),
        ("foo\nbar foo", "/foo<CR>0gNd"),
        ("foo\nbar foo", "/foo<CR>jgNd"),
    ]);
}

#[test]
fn gn_operators_and_dot_repeat() {
    check(&[
        (TEXT, "/foo<CR>0dgn"),
        (TEXT, "/foo<CR>0dgN"),
        (TEXT, "/foo<CR>0dgn."),
        (TEXT, "/foo<CR>0dgn.."),
        (TEXT, "/foo<CR>0cgnX<Esc>."),
        (TEXT, "/foo<CR>0cgnX<Esc>.."),
        (TEXT, "/foo<CR>0cgnX<Esc>0."),
        (TEXT, "/foo<CR>0cgnX<Esc>n."),
        (TEXT, "/foo<CR>0cgNX<Esc>."),
        (TEXT, "/foo<CR>0ygnyiw"),
        (TEXT, "/foo<CR>0d2gN"),
        (TEXT, "/foo<CR>0dgnu"),
        (TEXT, "/foo<CR>$dgN."),
        ("foo one\nfoo two\nfoo three", "/foo<CR>0cgnBAR<Esc>j0."),
        ("foo one\nfoo two\nfoo three", "/foo<CR>0cgnBAR<Esc>j0.j0."),
        ("foo bar\nbaz foo", "/foo<CR>0dgn."),
        ("a foo\nbar\nbaz", "/foo\\nbar<CR>0dgn"),
        ("a foo\nbar\nbaz", "/foo\\nbar<CR>0gnd"),
        ("a foo\nbar\nbaz", "/foo\\nbar<CR>0cgnX<Esc>"),
        ("foo\nbar\nbaz", "/foo\\nbar<CR>0dgn."),
    ]);
}

#[test]
fn gn_same_keys_on_several_texts() {
    let texts = [
        TEXT,
        "foo\nfoo\n\nfoo",
        "foofoo\n  foo foo\nfoo",
        "xfoo\n\nbar foo",
        "barbaz",
    ];
    check(&common::cross(
        &texts,
        &[
            "/foo<CR>0gnd",
            "/foo<CR>$gNd",
            "/foo<CR>G$vgNd",
            "/foo<CR>0vgngnd",
        ],
    ));
    // Repeats with "." only on texts where the first change succeeds: Vim
    // keeps the last change between cases, so a failed change would repeat
    // a change from another case.
    let with_foo = &texts[..4];
    check(&common::cross(
        with_foo,
        &["/foo<CR>0dgn.", "/foo<CR>0cgnX<Esc>.", "/foo<CR>$dgN."],
    ));
}

#[test]
fn gn_sets_desired_column() {
    // gn resets curswant to the cursor column it leaves; <Esc> keeps it, so
    // the vertical moves that follow aim at that column on longer and
    // shorter lines alike.
    check(&[
        ("a foo\nab\nabcdefgh", "/foo<CR>gn<Esc>jjX"),
        ("a foo\nab\nabcdefgh", "/foo<CR>0gn<Esc>jjX"),
        ("a foo\nab\nabcdefgh", "/foo<CR>$gN<Esc>jjX"),
        ("a foo\nab\nabcdefgh", "/foo<CR>gN<Esc>jjX"),
        ("a foo\nab\nabcdefgh", "/foo<CR>dgnjjX"),
        ("a foo\nab\nabcdefgh", "/foo<CR>gngn<Esc>kjX"),
    ]);
}

#[test]
fn gn_failure_leaves_cursor_and_text() {
    check(&[
        ("foo bar", "/zzz<CR>0gnx"),
        ("foo bar", "/zzz<CR>dgnx"),
        ("foo bar", "/zzz<CR>vgnd"),
        ("foo bar", "/zzz<CR>0gNx"),
        ("foo\nbar", "/foo<CR>:set nows<CR>Ggnx"),
        ("foo\nbar", "/foo<CR>:set nows<CR>Gdgnx"),
    ]);
}

#[test]
fn ex_read_errors_leave_buffer_alone() {
    check(&[
        ("a\nb", ":r /nonexistent_rvim_dir/none.txt<CR>x"),
        ("a\nb", "j:0r /nonexistent_rvim_dir/none.txt<CR>x"),
        ("a\nb", ":1r /nonexistent_rvim_dir/none.txt<CR>x"),
        ("a\nb", ":r<CR>x"),
        ("a\nb", ":99r /nonexistent_rvim_dir/none.txt<CR>x"),
    ]);
}
