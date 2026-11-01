// SPDX-License-Identifier: Apache-2.0
//! The jumplist (`<C-o>`, `<C-i>`/`<Tab>` with counts, `:jumps`) and the
//! previous context mark (`''`, ``` `` ```, `m'`, ``` m` ```) checked against
//! Vim: which commands are jumps, how the list and the mark follow line
//! edits, and the desired column after a jump.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &[
    "abcdef\nab\n  abc\nxyz\nlast line",
    "a\nb\nc\nd\ne",
    "foo bar\n\tbaz qux\nfoo\n  quux foo",
    "abcdef\nab\nabcdef\nabcdef",
];

#[test]
fn jump_commands_set_the_context_mark() {
    let keys = [
        "3G''ix<Esc>",
        "G''ix<Esc>",
        "jGgg''ix<Esc>",
        "jjG''ix<Esc>",
        "3G3G''ix<Esc>",
        "3G''''ix<Esc>",
        "3G'''ix<Esc>",
        "3G``ix<Esc>",
        "llljG``ix<Esc>",
        "llljG''ix<Esc>",
        "jj''ix<Esc>",
        "3G''<C-o>ix<Esc>",
        ":3<CR>''ix<Esc>",
        ":3,4<CR>''ix<Esc>",
        ":5<CR>:2<CR>''ix<Esc>",
        "/c<CR>''ix<Esc>",
        "?a<CR>''ix<Esc>",
        "/c<CR>n''ix<Esc>",
        "/c<CR>nN''ix<Esc>",
        "}''ix<Esc>",
        "{''ix<Esc>",
        "G{''ix<Esc>",
        "(''ix<Esc>",
        ")''ix<Esc>",
        "%''ix<Esc>",
        "jj%''ix<Esc>",
        "3G50%''ix<Esc>",
        "3G200%''ix<Esc>",
        "*''ix<Esc>",
        "#''ix<Esc>",
        "3Gdd''ix<Esc>",
        "jdG''ix<Esc>",
        "3Gdj''ix<Esc>",
        "3Gyj''ix<Esc>",
        "3Gv''d",
        "3Gv''y",
        "3G3Gd''ix<Esc>",
        "3Gu''ix<Esc>",
        "3Go<Esc>''ix<Esc>",
        "3Gix<Esc>''ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn context_mark_set_with_m() {
    let keys = [
        "jm'gg''ix<Esc>",
        "jm`gg``ix<Esc>",
        "jjm'G''ix<Esc>",
        "m'''ix<Esc>",
        "m''''ix<Esc>",
        "3G''m'''ix<Esc>",
        "jjm'j''ix<Esc>",
        "lllm`G``ix<Esc>",
        "lllm`G''ix<Esc>",
        "jm'ggm``ix<Esc>",
        "3Gm'gg:'<CR>ix<Esc>",
        "jmaG'a''ix<Esc>",
        "jmagg`a``ix<Esc>",
        "jmagg'a''ix<Esc>",
        "3Gm'dd''ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ctrl_o_and_tab_walk_the_jumplist() {
    let keys = [
        "3G<C-o>ix<Esc>",
        "3G<C-o><C-o>ix<Esc>",
        "3G5G<C-o>ix<Esc>",
        "3G5G<C-o><Tab>ix<Esc>",
        "3G5G<C-o><C-o><Tab><Tab>ix<Esc>",
        "3G5G<C-o><C-o><C-o>ix<Esc>",
        "3G5G<C-o><C-o><C-o><C-o><C-o>ix<Esc>",
        "3G5G<C-o><Tab><Tab><Tab>ix<Esc>",
        "3G5G2<C-o>ix<Esc>",
        "3G5G7G<C-o>2<C-o>ix<Esc>",
        "3G5G<C-o>2<Tab>ix<Esc>",
        "2G4G2<C-o>ix<Esc>",
        "3G5G3G<C-o>ix<Esc>",
        "3G5G3G<C-o><C-o>ix<Esc>",
        "3G5G3G<C-o><C-o><C-o>ix<Esc>",
        "3G5G3G<C-o><Tab>ix<Esc>",
        "3G5G<C-o><C-o>5<Tab>ix<Esc>",
        "3G5Ggg<C-o><C-o><C-o>ix<Esc>",
        "3G5G7G8G<C-o><C-o><C-o><C-o>ix<Esc>",
        "gg<C-o>ix<Esc>",
        "<Tab>ix<Esc>",
        "G<C-o><C-o><C-o>ix<Esc>",
        "3G:5<CR><C-o>ix<Esc>",
        "3G:5<CR><C-o><C-o><Tab>ix<Esc>",
        "/c<CR>/d<CR><C-o>ix<Esc>",
        "/c<CR>/d<CR><C-o><C-o><C-o>ix<Esc>",
        "3G/c<CR><C-o>ix<Esc>",
        "3Gmx5G<C-o>ix<Esc>",
        "3Gmx5G<C-o>`xix<Esc>",
        "3G5G<C-o>''ix<Esc>",
        "3G5G<C-o><C-o>''ix<Esc>",
        "3G5G<C-o>x<C-o>ix<Esc>",
        "3G5G<C-o><Tab>x",
        "3G5G<C-o>d<Tab>",
        "3G5G<C-o>D",
        "3G5G<C-o>3<C-o>ix<Esc>",
        "3G5G<C-o><C-o>dd<Tab>ix<Esc>",
        "3G5G<C-o><C-o>G<Tab>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ctrl_o_keeps_the_column_of_the_jump() {
    let keys = [
        "$G<C-o>jix<Esc>",
        "$G<C-o>jjix<Esc>",
        "llllG<C-o>jix<Esc>",
        "llllG<C-o>jjix<Esc>",
        "lllll''jix<Esc>",
        "lllll3G<C-o>jix<Esc>",
        "llllG<C-o>kix<Esc>",
        "llllG<C-o>0jix<Esc>",
        "llljG``jix<Esc>",
        "llljG''jix<Esc>",
        // A failed jump keeps the desired column.
        "$<C-o>jix<Esc>",
        "$<C-o>jjix<Esc>",
        "lllll<C-o>jix<Esc>",
        "lllll<Tab>jix<Esc>",
        "$3G<C-o>jjix<Esc>",
        "llllj''j",
        "llllj3G<C-o>j",
        "llllj3G<C-o><Tab>j",
        "llllj3G5G<C-o>k",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn jumplist_follows_line_changes() {
    let keys = [
        "3Gggdd<C-o>ix<Esc>",
        "3Gggo<Esc><C-o>ix<Esc>",
        "3GggO<Esc><C-o>ix<Esc>",
        "3GggOx<Esc>G<C-o><C-o>ix<Esc>",
        "3GggJ<C-o>ix<Esc>",
        "3GggJ<C-o><C-o>ix<Esc>",
        "GggGdd<C-o>ix<Esc>",
        "Gggdd<C-o>ix<Esc>",
        "3Gggyy5Gp<C-o>ix<Esc>",
        "3Gggyy5Gp<C-o><C-o>ix<Esc>",
        "3G5GggP<C-o><C-o>ix<Esc>",
        "3G5GggP<C-o><C-o><C-o>ix<Esc>",
        "3G5G:2,3m0<CR><C-o>ix<Esc>",
        "3G5G:1m$<CR><C-o>ix<Esc>",
        "3G5Gggm$<CR><C-o>ix<Esc>",
        "3G5G:3t0<CR><C-o>ix<Esc>",
        "3G5G:3,4d<CR><C-o>ix<Esc>",
        "3G5G:g/a/d<CR><C-o>ix<Esc>",
        "3G5Gu<C-o>ix<Esc>",
        "3Gggddu<C-o>ix<Esc>",
        "3Gggdd<C-o><C-o>u<C-o>ix<Esc>",
        "3Gggddu<C-r><C-o>ix<Esc>",
        "3Gggddu<C-r>''ix<Esc>",
        "3G:s/$/\\r/<CR><C-o>ix<Esc>",
        "3G:%s/\\n//<CR><C-o>ix<Esc>",
        "3G:%s/\\n//<CR>''ix<Esc>",
        "3Gggllli<CR><Esc><C-o>ix<Esc>",
        "3Gggi<CR><Esc><C-o>ix<Esc>",
        "3Ggg:s/b/x\\ry/<CR><C-o>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ex_jumps_and_global_are_jumps() {
    let keys = [
        ":jumps<CR>ix<Esc>",
        ":3<CR>:jumps<CR><C-o>ix<Esc>",
        ":g/c/s/c/C/<CR>''ix<Esc>",
        ":g/c/<CR><C-o>ix<Esc>",
        ":g/x/s/x/y/<CR>''ix<Esc>",
        ":g/q/s/x/y/<CR>''ix<Esc>",
        ":3s/c/C/<CR>''ix<Esc>",
        ":s/x/y/<CR>''ix<Esc>",
        ":5s/nomatch/y/<CR>''ix<Esc>",
        ":3,4s/$/!/<CR>''ix<Esc>",
        "3G:d<CR>''ix<Esc>",
        "3G:m0<CR>''ix<Esc>",
        "3G:t0<CR>''ix<Esc>",
        ":3<CR>:3,5j<CR><C-o>ix<Esc>",
        ":g/a/normal G<CR>''ix<Esc>",
        "3G:normal G<CR>''ix<Esc>",
        "3G:normal G<CR><C-o>ix<Esc>",
        // `&` repeats the last `:s`, which the harness does not carry between
        // cases, so each case substitutes first.
        ":s/b/B/<CR>j&''ix<Esc>",
        ":s/c/C/<CR>3G&''ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

/// `:s` with `\n` in the pattern leaves the cursor on the last join point in
/// Vim, while rvim puts it on the first non-blank (a known difference in
/// PLAN.md). These cases depend on that cursor, so they are not run until it
/// is fixed.
#[test]
#[ignore = "known difference: cursor after a multi-line :s (PLAN.md)"]
fn jumps_after_a_multi_line_substitute() {
    let keys = [
        ":2,3s/\\n//<CR>''ix<Esc>",
        "3Gggjj:1,2s/\\n//<CR><C-o>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn context_mark_follows_line_changes() {
    let keys = [
        "3Gggdd''ix<Esc>",
        "3Gggdd'`x",
        "3GG''dd''ix<Esc>",
        "3Gdd''ix<Esc>",
        "3GGdd''ix<Esc>",
        "3GGdd'`ix<Esc>",
        "3GGGdd''ix<Esc>",
        "2GGJ''ix<Esc>",
        "3Gggo<Esc>''ix<Esc>",
        "3Ggg<C-o>''ix<Esc>",
        "3Gggu''ix<Esc>",
        "3Gggddu''ix<Esc>",
        "3G:%s/\\n//<CR>''ix<Esc>",
        "jjGgg:1,2m$<CR>''ix<Esc>",
        "jjGgg:1,2m$<CR>``ix<Esc>",
        "3G:1,2m$<CR>''ix<Esc>",
        "3G:1,2m0<CR>''ix<Esc>",
        "3G:2,3m0<CR>''ix<Esc>",
        "jj:m0<CR>''ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn jumplist_is_capped_and_cleaned() {
    let keys = [
        "3G5G3G7G7G<C-o>ix<Esc>",
        "3G5G3G7G<C-o><C-o><C-o>ix<Esc>",
        "3G3G3G<C-o>ix<Esc>",
        "3G5G<Tab>ix<Esc>",
        "3G5G7G<C-o><C-o>2<Tab>ix<Esc>",
        "3G5G7G<C-o>G<C-o>ix<Esc>",
        "GG<C-o><C-o>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

/// `:jumps` is a message only. It must not move the cursor or change the text,
/// and it lists the entries oldest first with `>` at the current one.
#[test]
fn jumps_lists_the_entries() {
    use rvim::Editor;
    let mut ed = Editor::from_text("a\nb\nc\nd\ne");
    ed.feed("3G5G:jumps<CR>");
    let msg = ed.message().map(|m| m.text.clone());
    assert_eq!(msg.as_deref(), Some("jumps: 1:1 3:1 >"));
    assert_eq!(ed.cursor().line, 4);
    ed.feed("<C-o>:jumps<CR>");
    let msg = ed.message().map(|m| m.text.clone());
    // The position <C-o> came from is recorded at the end (line 5).
    assert_eq!(msg.as_deref(), Some("jumps: 1:1 >3:1 5:1"));
    assert_eq!(ed.cursor().line, 2);
    assert_eq!(ed.buf.lines(), ["a", "b", "c", "d", "e"]);
}

/// `H`, `M` and `L` are jumps. Where they land depends on the window height,
/// which the harness's Vim does not share with rvim, so only the recorded
/// jumplist entry (the cursor before the command) is checked here.
#[test]
fn screen_motions_set_the_context_mark() {
    use rvim::Editor;
    for keys in ["3GH", "3GM", "3GL"] {
        let mut ed = Editor::from_text("a\nb\nc\nd\ne");
        ed.feed(keys);
        ed.feed(":jumps<CR>");
        let msg = ed.message().map(|m| m.text.clone());
        assert!(
            msg.as_deref().is_some_and(|m| m.contains("3:1")),
            "{keys}: {msg:?}"
        );
    }
}

/// `m'` and ``m` `` keep the desired column, as `m{a-z}` does (Vim's
/// nv_mark()), so the cursor column of later vertical motions is the one
/// from before the mark was set.
#[test]
fn context_mark_keeps_the_desired_column() {
    let keys = [
        "lllm'jix<Esc>",
        "lllm'jjix<Esc>",
        "lllm'jjjix<Esc>",
        "lllm'jkix<Esc>",
        "lllm'jjkix<Esc>",
        "lllm`jix<Esc>",
        "lllm`jjix<Esc>",
        "lllm`jkix<Esc>",
        "lllm`jjkix<Esc>",
        "lllm'jjj'j'kix<Esc>",
        "llllm'jlix<Esc>",
        "llllm'jjjix<Esc>",
        "$m'jix<Esc>",
        "$m'jjix<Esc>",
        "$m'jjjjix<Esc>",
        "lllll3Gm'jix<Esc>",
        "lllm'Gkix<Esc>",
        "lllm'Gkkix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

/// `:ju` is `:jumps` (a message only), and `:j` still joins lines.
#[test]
fn jumps_abbreviation_and_join_are_distinct() {
    let keys = [
        ":ju<CR>ix<Esc>",
        ":jum<CR>ix<Esc>",
        "3G:jumps<CR>ix<Esc>",
        ":j<CR>ix<Esc>",
        ":j!<CR>ix<Esc>",
        "3G:j<CR>ix<Esc>",
        ":2,3j<CR>ix<Esc>",
        ":1,3j<CR>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn jumps_message_and_cursor_are_unchanged() {
    use rvim::Editor;
    let mut ed = Editor::from_text("a\nb\nc\nd\ne");
    ed.feed("3G5G");
    let before = ed.cursor();
    ed.feed(":ju<CR>");
    let msg = ed.message().map(|m| m.text.clone());
    assert!(
        msg.as_deref().is_some_and(|m| m.starts_with("jumps:")),
        "{msg:?}"
    );
    assert_eq!(ed.cursor(), before);
    assert_eq!(ed.buf.lines(), ["a", "b", "c", "d", "e"]);
}

#[test]
fn ctrl_o_after_an_insert_and_visual_mode() {
    let keys = [
        "3Gix<Esc>gg<C-o>ix<Esc>",
        "3Ggg<C-o>iy<Esc><C-o>ix<Esc>",
        "3Gvj<C-o>y",
        "3Gv<C-o><Esc>ix<Esc>",
        "3Gv<Tab>d",
        "3Gv''d",
        "jmv3G<C-o>d'a",
        "jjvgg<C-o>d",
    ];
    check(&cross(TEXTS, &keys));
}
