// SPDX-License-Identifier: Apache-2.0
//! Option scopes: `:setlocal` and `:setglobal` next to `:set`. A buffer-local
//! option has a global and a local value, and the local one is what the
//! buffer uses; a global-local option uses its local value only when it is
//! set. The harness resets the options before each case (`set opt&`, which
//! sets both values) and each case runs in a new buffer. The messages of
//! `:setl opt?` are not compared with Vim (the harness sees no messages); they
//! are checked in the unit tests at the end, against Vim 9.1 probes.

mod common;
use common::{check, cross};
use rvim::Editor;

const TEXTS: &[&str] = &[
    "foo\tbar\tbaz\nqux\n\tindented\tfoo\n  x\ty",
    "漢字 wide\tend\n語\tx漢\n\tlast 漢",
];

/// Indented lines for shifting and for `o` (autoindent).
const INDENT: &[&str] = &[
    "a\n\nfoo\n  \tx\n\t\ty z\n",
    "\tfoo bar\n      x\n   \tend\n",
];

/// Long enough to scroll, for 'scrolloff'.
const LONG: &[&str] = &[concat!(
    "l01\nl02\nl03\nl04\nl05\nl06\nl07\nl08\nl09\nl10\n",
    "l11\nl12\nl13\nl14\nl15\nl16\nl17\nl18\nl19\nl20\n",
    "l21\nl22\nl23\nl24\nl25\nl26\nl27\nl28\nl29\nl30\n",
    "l31\nl32\nl33\nl34\nl35\nl36\nl37\nl38\nl39\nl40"
)];

#[test]
fn setglobal_does_not_change_the_buffer_local_value() {
    // `:setglobal` leaves the current buffer's value alone: `>>` still uses
    // the local 'shiftwidth' of 8.
    check(&cross(
        INDENT,
        &[
            ":setglobal sw=4<CR>>>",
            ":setg sw=4<CR>>>",
            ":setg sw=4<CR>>>j>>",
            ":setg sw=2<CR>:set sw=3<CR>>>",
        ],
    ));
}

#[test]
fn setlocal_changes_only_the_buffer() {
    check(&cross(
        INDENT,
        &[
            ":setlocal sw=2<CR>>>",
            ":setl sw=2<CR>>>j>>",
            ":setl sw=2<CR>:setg sw=4<CR>>>",
            ":setl sw=2<CR>:setg sw=4<CR>:setl sw?<CR>>>",
            ":setl sw=3 et<CR>>>",
        ],
    ));
}

#[test]
fn setlocal_unset_copies_or_resets() {
    check(&cross(
        INDENT,
        &[
            // 'shiftwidth' copies the global value to the local one.
            ":setg sw=4<CR>:setl sw<<CR>>>",
            ":setl sw=2<CR>:setl sw<<CR>>>",
            ":setl sw=2<CR>:setg sw=5<CR>:set sw<<CR>>>",
            // `opt&` sets the local value to the default, the global stays.
            ":setg sw=4<CR>:setl sw=2<CR>:setl sw&<CR>>>",
            ":setg sw=4<CR>:setl sw&<CR>>>",
            ":set sw=3<CR>:set sw&<CR>>>",
            // `:setglobal opt<` does nothing to the buffer.
            ":setl sw=2<CR>:setg sw<<CR>>>",
        ],
    ));
}

#[test]
fn setlocal_arithmetic_starts_from_the_effective_value() {
    check(&cross(
        INDENT,
        &[
            ":setl sw+=2<CR>>>",
            ":setg sw=4<CR>:setl sw+=2<CR>>>",
            ":setg sw=4<CR>:setg sw-=1<CR>:setl sw^=2<CR>>>",
            ":setl sw=2<CR>:setg sw+=3<CR>>>",
            ":setl sw=2<CR>:setl sw-=1<CR>>>",
        ],
    ));
}

#[test]
fn setglobal_expandtab_and_autoindent_are_not_local() {
    check(&cross(
        INDENT,
        &[
            ":setg et<CR>>>",
            ":setl et<CR>>>",
            ":setg et<CR>:setl et!<CR>>>",
            ":setg ai<CR>jofoo<Esc>",
            ":setl ai<CR>jofoo<Esc>",
            ":setl ai!<CR>jofoo<Esc>",
            ":setl ai<CR>:setl invai<CR>jofoo<Esc>",
            ":setl ai<CR>:setl noai<CR>jofoo<Esc>",
        ],
    ));
}

#[test]
fn setlocal_softtabstop_and_smarttab() {
    check(&cross(
        INDENT,
        &[
            "i<Tab><Esc>",
            ":setg sts=4<CR>i<Tab><Esc>",
            ":setl sts=4<CR>i<Tab><Esc>",
            ":setl sta sw=4<CR>i<Tab><Esc>",
            ":setg sta sw=4<CR>i<Tab><Esc>",
            ":setl sw=4<CR>:setl sta<CR>jI<Tab><Esc>",
        ],
    ));
}

#[test]
fn setlocal_tabstop_changes_display_columns() {
    let keys = [
        ":setg ts=2<CR>j3|x",
        ":setg ts=2<CR>jj$",
        ":setl ts=2<CR>j3|x",
        ":setl ts=2<CR>jj$",
        ":setl ts=2<CR>:setg ts=4<CR>j3|x",
        ":setl ts=4<CR>:setl ts<<CR>j3|x",
        ":setglobal ts=4<CR>:setlocal ts=2<CR>jjlll|x",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn setglobal_is_seen_by_the_search() {
    // 'ignorecase' and 'hlsearch' are global: all three forms agree.
    let keys = [
        ":setg ic<CR>/FOO<CR>x",
        ":setl ic<CR>/FOO<CR>x",
        ":setlocal ic<CR>/FOO<CR>x",
        ":setglobal ic<CR>/FOO<CR>x",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn setlocal_global_option_sets_the_global_value() {
    // 'shiftround' is global: `:setl sr` changes the value `>` uses.
    check(&cross(
        INDENT,
        &[
            ":setl sw=4 sr<CR>>>",
            ":setl sw=4<CR>:setg sr<CR>>>",
            ":setl sw=4<CR>:set sr<CR>>>",
        ],
    ));
}

#[test]
fn setlocal_scrolloff_is_window_local() {
    // 'scrolloff' is global-local to the window: the local value wins when it
    // is set, the global value is used otherwise.
    let keys = [
        ":setl so=5<CR>10<C-e>",
        ":setg so=5<CR>10<C-e>",
        ":set so=5<CR>10<C-e>",
        ":setl so=5<CR>:setl so<<CR>10<C-e>",
        ":setl so=5<CR>:setg so=0<CR>10<C-e>",
        ":setg so=5<CR>:setl so=2<CR>10<C-e>",
        ":setg so=5<CR>:setl so=2<CR>:setg so=0<CR>10<C-e>",
    ];
    check(&cross(LONG, &keys));
}

#[test]
fn setlocal_and_set_forms_agree() {
    let keys = [
        ":setlocal sw=2 sts=2 et<CR>>>",
        ":setl sw=2 sts=2 et<CR>>>",
        ":setglobal sw=2 sts=2 et<CR>>>",
        ":setglob sw=2 sts=2 et<CR>>>",
        ":setl sw=2 sts=2 et<CR>:set sw&<CR>>>",
        ":setg sw=2<CR>:setl ts=3<CR>j$",
    ];
    check(&cross(INDENT, &keys));
}

/// The last message after running `keys` in an editor on `text`.
fn msg(text: &str, keys: &str) -> String {
    let mut ed = Editor::from_text(text);
    ed.feed(keys);
    ed.message().map(|m| m.text.clone()).unwrap_or_default()
}

/// The messages of `:setl opt?`, `:setg opt?` and `:set opt?` after each
/// setup, as Vim 9.1 prints them (probed with `vim -N -u NONE -es`).
#[test]
fn setlocal_and_setglobal_queries() {
    assert_eq!(msg("x\n", ":setl sw=2<CR>:setl sw?<CR>"), "  shiftwidth=2");
    assert_eq!(msg("x\n", ":setl sw=2<CR>:setg sw?<CR>"), "  shiftwidth=8");
    assert_eq!(msg("x\n", ":setl sw=2<CR>:set sw?<CR>"), "  shiftwidth=2");
    assert_eq!(msg("x\n", ":setg sw=4<CR>:set sw?<CR>"), "  shiftwidth=8");
    assert_eq!(msg("x\n", ":setg sw=4<CR>:setg sw?<CR>"), "  shiftwidth=4");
    assert_eq!(msg("x\n", ":setg sw?<CR>"), "  shiftwidth=8");
    assert_eq!(msg("x\n", ":setl sw?<CR>"), "  shiftwidth=8");
    assert_eq!(msg("x\n", ":setl ai?<CR>"), "noautoindent");
    // A query in a `:setglobal` command shows the global value.
    assert_eq!(msg("x\n", ":setglobal ai ai?<CR>"), "  autoindent");
    // The buffer's local value is still off, so `:set ai?` shows that.
    assert_eq!(msg("x\n", ":setglobal ai<CR>:set ai?<CR>"), "noautoindent");
}

#[test]
fn setlocal_scrolloff_and_autoread_show_unset_values() {
    // An unset global-local value: `-1` for a number, `--` for a boolean.
    assert_eq!(msg("x\n", ":setl so?<CR>"), "  scrolloff=-1");
    assert_eq!(msg("x\n", ":setg so?<CR>"), "  scrolloff=0");
    assert_eq!(msg("x\n", ":set so?<CR>"), "  scrolloff=0");
    assert_eq!(msg("x\n", ":setl so=3<CR>:setl so?<CR>"), "  scrolloff=3");
    assert_eq!(
        msg("x\n", ":setl so=3<CR>:setg so=7<CR>:set so?<CR>"),
        "  scrolloff=3"
    );
    assert_eq!(msg("x\n", ":setl ar?<CR>"), "--autoread");
    assert_eq!(msg("x\n", ":setg ar?<CR>"), "noautoread");
    assert_eq!(msg("x\n", ":setl ar<CR>:setl ar?<CR>"), "  autoread");
    assert_eq!(
        msg("x\n", ":setl ar<CR>:setl ar<<CR>:setl ar?<CR>"),
        "--autoread"
    );
    assert_eq!(msg("x\n", ":setl ar<CR>:set ar?<CR>"), "  autoread");
    assert_eq!(msg("x\n", ":setl nu?<CR>"), "nonumber");
    assert_eq!(
        msg("x\n", ":setl nu<CR>:setg nonu<CR>:setl nu?<CR>"),
        "  number"
    );
}

#[test]
fn setlocal_and_setglobal_errors_as_in_vim() {
    assert!(msg("x\n", ":setl nosw<CR>").starts_with("E474"));
    assert!(msg("x\n", ":setg nosw<CR>").starts_with("E474"));
    assert_eq!(msg("x\n", ":setl foo=1<CR>"), "E518: Unknown option: foo=1");
    assert_eq!(msg("x\n", ":setg foo=1<CR>"), "E518: Unknown option: foo=1");
    assert!(msg("x\n", ":setl sw=abc<CR>").starts_with("E521"));
    assert!(msg("x\n", ":setg sw=abc<CR>").starts_with("E521"));
    assert!(msg("x\n", ":setl ic=1<CR>").starts_with("E474"));
    assert!(msg("x\n", ":setg sw=-1<CR>").starts_with("E487"));
    assert!(msg("x\n", ":setl sw=-1<CR>").starts_with("E487"));
    // Vim accepts a negative 'scrolloff' (and 'softtabstop').
    assert!(msg("x\n", ":setl so=-5<CR>").is_empty());
    assert_eq!(msg("x\n", ":setl so=-5<CR>:setl so?<CR>"), "  scrolloff=-5");
    assert!(msg("x\n", ":setl sts=-2<CR>").is_empty());
}
