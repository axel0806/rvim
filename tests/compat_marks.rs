// SPDX-License-Identifier: Apache-2.0
//! Lowercase marks checked against Vim: `m{a-z}`, `'a` and `` `a `` (also as
//! operator motions and Visual ends), `:k`/`:mark`, `'a` as an Ex address,
//! and how marks follow line and column changes, undo and `:m`.

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &[
    "abcdef\nab\n  abc\nxyz\nlast line",
    "a\nb\nc\nd\ne",
    "foo bar\n\tbaz qux\nfoo\n  quux foo",
    "abcdef\nab\nabcdef\nabcdef",
];

#[test]
fn set_and_jump() {
    let keys = [
        "jmagg'aD",
        "jllmagg`aix<Esc>",
        "lllmaj0`ax",
        "lllmaj0`aix<Esc>",
        "jmzgg'zix<Esc>",
        "jmzgg`zix<Esc>",
        "jmagg'a",
        "jmagg`a",
        "jmajmbgg'bix<Esc>'aix<Esc>",
        "jmaggO<Esc>'aix<Esc>",
        "jjmaggOx<Esc>'aD",
        "lll`ax",
        "lll'ax",
        "'ax",
        "jllmaggjjx`aix<Esc>",
        "ma'aix<Esc>",
        "jmau'aix<Esc>",
        "jlllmamaggi`aix<Esc>",
        "jmaggjdd2'aix<Esc>",
        "jmagg3`aix<Esc>",
        // The desired column: a jump resets it, a failed jump or a
        // motion with no mark moves nothing.
        "lllljma'aj",
        "lllljma`aj",
        "llllj:ka<CR>j",
        "llllj:ka<CR>k",
        "llllj'aj",
        "lllljmakkj",
        // Setting a mark keeps the desired column.
        "lllljmajix<Esc>",
        "lllljmajjix<Esc>",
        "lllljmakkjix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn operators_take_marks_as_motions() {
    let keys = [
        "lllmaj0d`a",
        "lllmaj0d'a",
        "lllmaj0y`aP",
        "jmajdd`aix<Esc>",
        "jmadd'aix<Esc>",
        "jmaddpgg'aix<Esc>",
        "jjmajd'aix<Esc>",
        "jjmajd`aix<Esc>",
        "jjmakd'aix<Esc>",
        "jjjmakkkd'aix<Esc>",
        "jjmajjdk'aix<Esc>",
        "jjmaggy'aix<Esc>",
        "jjmaggdd'aix<Esc>",
        "jjmaggdd`aix<Esc>",
        "jjmaggdddd'aix<Esc>",
        "jmajd'a.",
        "jllmaggdj`aix<Esc>",
        "jllmaggdj'aix<Esc>",
        "jjmaggVjd`aix<Esc>",
        "jjmaggVj`aix<Esc>",
        "jjmaggvjd'aix<Esc>",
        "jjlmaggvjd`aix<Esc>",
        "jjlmaggvj'aix<Esc>",
        "jjlmaggvj`aix<Esc>",
        "jjlvmagg`aix<Esc>",
        "jjlvmagg`aix<Esc>",
        "jllmaggvj'aix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ex_mark_and_addresses() {
    let keys = [
        "jmagg:'a<CR>ix<Esc>",
        ":3ka<CR>gg:'a<CR>ix<Esc>",
        ":mark a<CR>ggj:'a<CR>ix<Esc>",
        ":ma b<CR>gg:'b<CR>ix<Esc>",
        ":k a<CR>gg:'a<CR>ix<Esc>",
        ":3,4k a<CR>gg:'a<CR>ix<Esc>",
        ":k 1<CR>ix<Esc>",
        ":k<CR>ix<Esc>",
        ":ka b<CR>ix<Esc>",
        ":'a<CR>ix<Esc>",
        "gg:'a,'bd<CR>ix<Esc>",
        "jmajmbgg:'a,'bd<CR>ix<Esc>",
        "jmajjmb:'a,'bd<CR>ix<Esc>",
        "jmajjmb:'a,'bs/$/x/<CR>ix<Esc>",
        "jmajjmb:'a,.s/$/x/<CR>ix<Esc>",
        "jmajjmb:'a,'bm0<CR>ix<Esc>",
        "jmaj:'a,.m0<CR>'aix<Esc>",
        "jjmaggmb:'a<CR>ix<Esc>",
        "jma:'a+1d<CR>ix<Esc>",
        "jma:'a-1d<CR>'aix<Esc>",
        "jjma:'a,$d<CR>ix<Esc>",
        "jmakkjjmb:'a,'bj<CR>ix<Esc>",
        "jjjmakk:2,3m0<CR>'aix<Esc>",
        "jjmak:'a,'a+1m$<CR>'aix<Esc>",
        "jjmak:'a,.-1m$<CR>'aix<Esc>",
        "jjmak:2,3m0<CR>'aix<Esc>",
        "jjmak:2,3t0<CR>'aix<Esc>",
        "jmaggyy:'ap<CR>'aix<Esc>",
        "jmak:2,3j<CR>'aix<Esc>",
        "jmak:2,3j<CR>`aix<Esc>",
        "jmak:g/a/d<CR>'aix<Esc>",
        "jjma:'a<CR>:'a,$s/$/!/<CR>ix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn marks_follow_line_changes() {
    let keys = [
        "jjmaggOx<Esc>'aix<Esc>",
        "jjmaggox<Esc>'aix<Esc>",
        "jmaooX<Esc>'aix<Esc>",
        "jmaOX<Esc>'aix<Esc>",
        "jjmaggyyp'aix<Esc>",
        "jmayyggp'aix<Esc>",
        "jmaggyyjp'aix<Esc>",
        "jmaggyyP'aix<Esc>",
        "jmaggyyjP'aix<Esc>",
        "jjmakJ'aix<Esc>",
        "jmaJgg'aix<Esc>",
        "jjmaJ'aix<Esc>",
        "jjmaJ`aix<Esc>",
        "jjlmakJ`aix<Esc>",
        "jjlmakJ'aix<Esc>",
        "jjlmakgJ`aix<Esc>",
        "jjlmaggJ`aix<Esc>",
        "jjlmaggJggJ`aix<Esc>",
        "jllmaggJ`aix<Esc>",
        "jlllmaggJ`aix<Esc>",
        "jllllmaggJ`aix<Esc>",
        "jjjlmakkgJ`aix<Esc>",
        "jjjlmaggJ`aix<Esc>",
        "jjjlmaggkJ`aix<Esc>",
        "jjjlmaggJ`aix<Esc>",
        "jjjlmagg3J`aix<Esc>",
        "jjmaggdd'a",
        "jjmaggdd`aix<Esc>",
        "jjmaggdd:'a<CR>ix<Esc>",
        "jjmaggdddd:'a<CR>ix<Esc>",
        "jjmaggdd:'a,$d<CR>ix<Esc>",
        "jjmaggdd:'a<CR>ix<Esc>",
        "jjmakI<BS><Esc>'aix<Esc>",
        "jjmakI<BS><Esc>`aix<Esc>",
        "jjlmakI<BS><Esc>`aix<Esc>",
        "jjlmaggA<Del><Esc>`aix<Esc>",
        "jjlmaggA<Del><Esc>'aix<Esc>",
        "jjmaggA<Del><Esc>'aix<Esc>",
        "jjlmaggi<CR><Esc>'aix<Esc>",
        "jjlmaggi<CR><Esc>`aix<Esc>",
        "jjlmaggi<CR><Esc>`aix<Esc>",
        "llllmaggi<CR><Esc>`ax",
        "llllmagglli<CR><Esc>`ax",
        "llllmaggiX<Esc>`ax",
        "llllmaggx`aix<Esc>",
        "llllmagg2x`aix<Esc>",
        "llllmaggllx`aix<Esc>",
        "llllmaggllD`aix<Esc>",
        "llllmaggllCQ<Esc>`aix<Esc>",
        "llllmaggll~`aix<Esc>",
        "llllmaggllrq`aix<Esc>",
        "llllmaggllsQ<Esc>`aix<Esc>",
        "llllmaggll\"_dl`aix<Esc>",
        "jjmak:%s/\\n//<CR>'aix<Esc>",
        "jjmak:%s/\\n//<CR>`aix<Esc>",
        "jjmak:1,2s/\\n//<CR>'aix<Esc>",
        "jjmak:1,2s/\\n//<CR>`aix<Esc>",
        "jmak:s/$/\\r/<CR>'aix<Esc>",
        "jmak:s/$/\\r/<CR>`aix<Esc>",
        "llmaj:s/c/\\r/<CR>`aix<Esc>",
        "jjmaggvjyGP'aix<Esc>",
        "jjmaggvjyGp'aix<Esc>",
        "jjmaggvjyjjp'aix<Esc>",
        "jjmaggvjyjjp`aix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn undo_and_redo_restore_marks() {
    let keys = [
        "jmaddu'aix<Esc>",
        "jmaddu`aix<Esc>",
        "jmaddu:'a<CR>ix<Esc>",
        "jmaddu<C-r>'aix<Esc>",
        "jmaddu<C-r>`aix<Esc>",
        "jjmaggOx<Esc>juu'aix<Esc>",
        "jjmaggdduu:'a<CR>ix<Esc>",
        "jjmaggdd'aixuu`aix<Esc>",
        "jjmaggdd<C-r>'aix<Esc>",
        "jjmak:2,3j<CR>u'aix<Esc>",
        "jjmak:2,3m0<CR>u'aix<Esc>",
        "jjmak:2,3m0<CR>u`aix<Esc>",
        "jjmak:2,3m0<CR>u<C-r>'aix<Esc>",
        "jjmak:2,3m0<CR>uu'aix<Esc>",
        "jjmak:2,3m0<CR>uu`aix<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn marks_in_visual_mode() {
    let keys = [
        "jjmvgg`aix<Esc>",
        "vjmagg`aix<Esc>",
        "vjmaggd'aix<Esc>",
        "jlvjmaggd`aix<Esc>",
        "jjlvmaggy`aP",
    ];
    check(&cross(TEXTS, &keys));
}
