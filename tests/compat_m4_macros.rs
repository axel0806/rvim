//! Macros (`q`, `@`) checked against Vim, including the register contents
//! that recording leaves behind. (The harness runs Vim under `silent!`, where
//! an error does not flush typeahead, so macros never abort there; abort on
//! error is checked by `tests/macros.rs` instead.)

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &["foo bar baz\nqux foo quux\n\n  indented foo line\nlast foo"];

#[test]
fn record_and_replay() {
    let keys = [
        "qaq",
        "qaxq",
        "qaxq@a",
        "qaxq3@a",
        "qaxjq@a@@",
        "qaxjq@a@@@@",
        "qa0xjq@a@@@@",
        "qadwq@a",
        "qadwq3@a",
        "qadwq10@a",
        "qaA!<Esc>jq@a",
        "qaA!<Esc>jq2@a",
        "qaxqj@a",
        "qaxqj@aj@a",
        "qa2xq@a",
        "qaJq3@a",
        "qaJq@a@@",
        "qa/foo<CR>q@a",
        "qa/foo<CR>q@a@@",
        "qa/foo<CR>xq@a",
        "qaofoo<Esc>q@a",
        "qaofoo<Esc>q3@a",
        "qaOfoo<Esc>jq@a",
        "qa:s/o/0/<CR>jq@a",
        "qaxjxq@au",
        "qaxjxq@au@a",
        "qaxq:2<CR>@a",
        "qaxqqAxq",
        "qaxqqAjxq@a",
        "qaxqqbjq@a",
        "qaxqqbq",
        "qaxqqAq",
        "qaxqqbdwq@a@b",
        "qa<Esc>q",
        "qaiX<Esc>q@a",
        "qaiX<Esc>jq@a",
        "qaiX<Esc>jq2@a",
        "qa~q@a",
        "qa~lq3@a",
        "qaq@a",
        "@a",
        "@@",
        "j@:",
        ":s/o/0/<CR>j@:",
        ":s/o/0/<CR>j@:j@@",
        ":s/o/0/g<CR>j2@:",
        "ix<Esc>\"ayl@a",
        "ixx<Esc>0\"ayiw@a",
        "yy@\"",
        "\"ayy@a",
        "qaxq\"ap",
        "qaxq\"Ayl\"ap",
        "qa\"byyq",
        "qa:2<CR>q@a",
        "qaGq@a",
        "qagg:3<CR>xq@a",
        "qaq2@a",
        "qa3xq2@a",
        "qa3xq@a@@",
    ];
    check(&cross(TEXTS, &keys));
}
