// SPDX-License-Identifier: Apache-2.0
//! With the agent opt-in off, `\a`, `\al` and `\ac` are Vim's keys: `\` does
//! nothing and the key after it is a command of its own (PLAN.md, "Agent
//! experience", "Keys and commands"; CLAUDE.md, extensions and the Vim
//! contract). Checked against real Vim by the differential harness.

mod common;
use common::{check, cross};

#[test]
fn desired_column_around_the_leader_keys_with_the_layer_off() {
    // CLAUDE.md: `lllljXj`-style cases. With the layer off, `\` is Vim's
    // invalid key and the key after it is a command of its own, so the
    // column the `j` set must survive the `\` exactly as Vim keeps it.
    let texts = ["abcdefgh\nab\nabcdefgh\nabcdefgh\n"];
    let keys = [
        "lllj\\jX<Esc>",
        "lllj\\jjX<Esc>",
        "$j\\kX<Esc>",
        "lllj2\\jX<Esc>",
        "lllj\\ljX<Esc>",
        "lllj\\<Esc>jX<Esc>",
        "lllj\\\\jX<Esc>",
    ];
    check(&cross(&texts, &keys));
}

#[test]
fn leader_agent_keys_are_vims_keys_with_the_layer_off() {
    let keys = [
        "\\aZ<Esc>",
        "\\al<Esc>",
        "\\ac<Esc>",
        "$\\a<Esc>",
        "\\\\aZ<Esc>",
    ];
    check(&cross(&["abc\n", "a\nb\n"], &keys));
}
