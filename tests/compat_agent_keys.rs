// SPDX-License-Identifier: Apache-2.0
//! With the agent opt-in off, `\a`, `\al` and `\ac` are Vim's keys: `\` does
//! nothing and the key after it is a command of its own (PLAN.md, "Agent
//! experience", "Keys and commands"; CLAUDE.md, extensions and the Vim
//! contract). Checked against real Vim by the differential harness.

mod common;
use common::{check, cross};

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
