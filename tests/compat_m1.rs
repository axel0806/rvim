// SPDX-License-Identifier: Apache-2.0
//! Milestone 1 behaviour checked against real Vim (see tests/common).

mod common;
use common::{check, cross};

const TEXTS: &[&str] = &[
    "foo.bar  baz\n\nqux, quux(1)\n\t indented\tline\n  \nlast_word",
    "a\tb漢字c d\nxy\n\t\tz\nlong line here with many words in it\n",
    "",
    "single",
    "  lead\n(x)",
];

#[test]
fn motions() {
    let keys = [
        "w",
        "3w",
        "20w",
        "W",
        "3W",
        "e",
        "4e",
        "20e",
        "E",
        "3E",
        "b",
        "Gb",
        "G$b",
        "G$4b",
        "GB",
        "G$20b",
        "G$e",
        "Gw",
        "j",
        "3j",
        "jjj",
        "20j",
        "k",
        "Gk",
        "G3k",
        "l",
        "5l",
        "$l",
        "h",
        "$3h",
        "0",
        "$0",
        "^",
        "jj^",
        "jjj^",
        "$",
        "2$",
        "3$",
        "20$",
        "G",
        "2G",
        "99G",
        "gg",
        "Ggg",
        "3gg",
        "+",
        "2+",
        "<CR>",
        "-",
        "G-",
        "G2-",
        "<Space>",
        "5<Space>",
        "$<Space>",
        "<BS>",
        "j<BS>",
        "jj<BS><BS>",
        "<Right>",
        "<Left>",
        "<Down>",
        "G<Up>",
        "<End>",
        "$<Home>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn unicode_word_classes() {
    let texts = [
        "漢字かなカナ😀😀 x²³y — z…w\u{2003}v ab한글cd",
        "αβγ;δ ① ⠁⠂ ∑x→y",
        "😀a😀 b©c ‼d",
    ];
    let keys = [
        "w", "2w", "3w", "4w", "5w", "6w", "7w", "8w", "9w", "10w", "12w", "14w", "e", "3e", "6e",
        "9e", "$b", "$3b", "$6b", "W", "2W", "E",
    ];
    check(&cross(&texts, &keys));
}

#[test]
fn curswant() {
    let keys = [
        "$jj",
        "$jjk",
        "lllljjj",
        "4ljjjkk",
        "jj$kk",
        "$j0j",
        "w5ljjj",
        "lllljxk",
        "llllj<Esc>j",
        "lllll2j",
        "lllllj3j",
        "lllljhj",
        "lllljlj",
        "lllljxj",
        "llllj:2<CR>j",
        "lllljiX<Esc>j",
        "lllljaX<Esc>j",
        "llllj<C-g>j",
        "llllj5<Del>j",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn deletes() {
    let keys = [
        "x", "3x", "$x", "jx", "10x", "jjx", "llljx", "x<Del>", "$xx", "Gx", "3<Del>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn inserts() {
    let keys = [
        "ihello<Esc>",
        "ahello<Esc>",
        "Ahello<Esc>",
        "Ihello<Esc>",
        "jjjIx<Esc>",
        "jjjjIx<Esc>",
        "ofoo<Esc>",
        "Ofoo<Esc>",
        "Gofoo<Esc>",
        "3ix<Esc>",
        "3ifoo<Esc>",
        "3ofoo<Esc>",
        "3Ofoo<Esc>",
        "2Aab<Esc>",
        "ia<CR>b<Esc>",
        "3ia<CR>b<Esc>",
        "i<CR><Esc>",
        "o<Esc>",
        "jjo<Esc>",
        "jI<BS><BS>x<Esc>",
        "i<BS>x<Esc>",
        "jja<Del><Del><Esc>",
        "A<Del><Esc>",
        "GA<Del><Esc>",
        "ifoo<Left>X<Esc>",
        "3ifoo<Left>X<Esc>",
        "ifoo<Esc>j",
        "A<Tab>x<Esc>",
        "jA<Up>X<Esc>",
        "$i<Down><Down>X<Esc>",
        "ix<Esc>jj",
        "Ax<Esc>jj",
        "iab<BS><BS><BS>c<Esc>",
        "i<End>Z<Esc>",
        "$i<Home>Z<Esc>",
        "ia<C-j>b<Esc>",
    ];
    check(&cross(TEXTS, &keys));
}

#[test]
fn ex_addresses() {
    let keys = [
        ":3<CR>", ":$<CR>", ":.+2<CR>", "G:-2<CR>", ":0<CR>", ":99<CR>", ":+<CR>", "G:.<CR>",
    ];
    check(&cross(TEXTS, &keys));
}
