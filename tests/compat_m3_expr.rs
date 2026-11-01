// SPDX-License-Identifier: Apache-2.0
//! `:s/pat/\=expr/`: the replacement evaluated as an expression, checked
//! against real Vim. The cases cover the expression language of `src/expr.rs`
//! through the substitute, and the errors and their effect on the `:s`.
//! In the keys, `<lt>` is a literal `<`, and `@` is the delimiter where the
//! expression has `#` or `/` in it.

mod common;
use common::{check, cross};

#[test]
fn replacement_is_evaluated_per_match() {
    check(&[
        ("abc", r#":s/a/\="x"/<CR>"#),
        ("abc", r#":s/b/\=submatch(0)/<CR>"#),
        ("ab", r#":s/\(a\)\(b\)/\=submatch(2).submatch(1)/<CR>"#),
        ("a1 b22", r#":s/\d\+/\=submatch(0)*2/g<CR>"#),
        ("x x x", r#":s/x/\=col('.')/g<CR>"#),
        ("ab\ncd", r#":%s/./\=line('.')/g<CR>"#),
        ("abcb", r#":s/b/\=line('.').':'.col('.')/g<CR>"#),
        ("ab cd", r#":s/\w\+/\=toupper(submatch(0))/g<CR>"#),
        ("abcdef", r#":s/cd/\=line('$')/<CR>"#),
        ("a\nb\nc", r#":%s/a\nb/\=submatch(0).'X'/<CR>"#),
        ("abcb", r#":s/\(b\)/\=submatch(1)/g<CR>"#),
        ("a1b", r#":s/\d/\=submatch(0)+1/<CR>"#),
        ("ab", r#":s/a\zsb/\=col('.')/<CR>"#),
        ("ab", r#":s/a\zsb/\=line('.')/<CR>"#),
        ("x", r#":s/x/\=7/<CR>"#),
        ("x", r#":s@x@\=7@<CR>"#),
        ("x\nx", r#":%s@x@\=''@<CR>"#),
    ]);
}

#[test]
fn result_text_has_line_breaks_and_no_escapes() {
    check(&[
        ("x", r#":s/x/\="a\nb"/<CR>"#),
        ("x", r#":s/x/\="a\rb"/<CR>"#),
        ("x", r#":s@x@\=['a','b']@<CR>"#),
        ("x", r#":s@x@\=[]@<CR>"#),
        ("x", r#":s@x@\=[1,[2]]@<CR>"#),
        ("x", r#":s/x/\="&"/<CR>"#),
        ("x", r#":s/x/\="\1"/<CR>"#),
        ("x", r#":s@x@\=''/''@<CR>"#),
        ("x", r#":s@x@\='a\/b'@<CR>"#),
        ("x", r#":s/x/\='\\'/<CR>"#),
        ("x", r#":s@x@\=7%3@<CR>"#),
        ("x", r#":s@x@\=''@<CR>"#),
        ("x", r#":s@x@\=[1,2,]@<CR>"#),
        ("x", r#":s@x@\= 1 + 1 @<CR>"#),
        ("x", r#":s@x@\=1 + 1 <CR>"#),
    ]);
}

#[test]
fn arithmetic_and_logic() {
    check(&[
        ("x", r#":s@x@\=1+2*3@<CR>"#),
        ("x", r#":s@x@\=-7/2@<CR>"#),
        ("x", r#":s@x@\=-7%3 . 7%-3@<CR>"#),
        ("x", r#":s@x@\=1/0@<CR>"#),
        ("x", r#":s@x@\=-1/0@<CR>"#),
        ("x", r#":s@x@\=0/0@<CR>"#),
        ("x", r#":s@x@\=7%0@<CR>"#),
        ("x", r#":s@x@\=1 ? 'y' : 'n'@<CR>"#),
        ("x", r#":s@x@\=1 ? 2 : 3 ? 4 : 5@<CR>"#),
        ("x", r#":s@x@\=0 ? 2 : 0 ? 4 : 5@<CR>"#),
        ("x", r#":s@x@\=0 && 1@<CR>"#),
        ("x", r#":s@x@\=0 || 2@<CR>"#),
        ("x", r#":s@x@\=!0 . !5@<CR>"#),
        ("x", r#":s@x@\=!!3 . !''@<CR>"#),
        ("x", r#":s@x@\=-(3)@<CR>"#),
        ("x", r#":s@x@\=5 - -3 . 2*-3 . !!3@<CR>"#),
        ("x", r#":s@x@\=1 . 2 + 3@<CR>"#),
        ("x", r#":s@x@\=1 + 2 . 3@<CR>"#),
        ("x", r#":s@x@\=1 == 1 && 2 > 1@<CR>"#),
        ("x", r#":s@x@\=2 + 3 == 5@<CR>"#),
        ("x", r#":s@x@\='a' . 1 == 'a1'@<CR>"#),
        ("x", r#":s@x@\=017 . 0x1F@<CR>"#),
        ("x", r#":s@x@\='3x' + 1 . 'a' + 1@<CR>"#),
        ("x", r#":s@x@\=-'3' . 1@<CR>"#),
        ("x", r#":s@x@\=0 ? foo : 'y'@<CR>"#),
        ("x", r#":s@x@\=1 ? 'y' : foo@<CR>"#),
        ("x", r#":s@x@\=0 && foo@<CR>"#),
        ("x", r#":s@x@\=1 || foo@<CR>"#),
    ]);
}

#[test]
fn comparisons() {
    check(&[
        ("x", r#":s@x@\='a' == 'A'@<CR>"#),
        ("x", r#":s@x@\='a' ==? 'A'@<CR>"#),
        ("x", r#":s@x@\='a' ==# 'A'@<CR>"#),
        ("x", r#":s@x@\='a' !=# 'A'@<CR>"#),
        ("x", r#":s@x@\='b' >=# 'a'@<CR>"#),
        ("x", r#":s@x@\='a' >? 'A'@<CR>"#),
        ("x", r#":s@x@\=1 == '1'@<CR>"#),
        ("x", r#":s@x@\='10' <lt> '9'@<CR>"#),
        ("x", r#":s@x@\=10 <lt> 9@<CR>"#),
        ("x", r#":s@x@\='abc' <lt> 'abd'@<CR>"#),
        ("x", r#":s@x@\='a' <lt> 1@<CR>"#),
        ("x", r#":s@x@\=1 != 2@<CR>"#),
        ("x", r#":s@x@\=1 >= 1 . 2 <lt>= 1@<CR>"#),
        ("x", r#":s@x@\=[1] == [1]@<CR>"#),
        ("x", r#":s@x@\=[1] <lt> [2]@<CR>"#),
        ("x", r#":s@x@\='abc' =~ 'b'@<CR>"#),
        ("x", r#":s@x@\='abc' !~ 'b'@<CR>"#),
        ("x", r#":s@x@\='ABC' =~ 'b'@<CR>"#),
        ("x", r#":s@x@\='ABC' =~? 'b'@<CR>"#),
        ("x", r#":s@x@\='ABC' =~# 'b'@<CR>"#),
        ("x", r#":s@x@\='a' !~# 'A'@<CR>"#),
        ("x", r#":s@x@\=!''@<CR>"#),
        ("x", r#":set ic<CR>:s@x@\='a' == 'A'@<CR>"#),
        ("x", r#":set ic<CR>:s@x@\='a' ==# 'A'@<CR>"#),
    ]);
}

#[test]
fn lists_and_indexing() {
    check(&[
        ("x", r#":s@x@\=[1,2,3][1]@<CR>"#),
        ("x", r#":s@x@\=[1,2,3][-1]@<CR>"#),
        ("x", r#":s@x@\='abc'[1]@<CR>"#),
        ("x", r#":s@x@\='abc'[-1]@<CR>"#),
        ("x", r#":s@x@\=[1,2,3][1:2]@<CR>"#),
        ("x", r#":s@x@\='abcdef'[2:3]@<CR>"#),
        ("x", r#":s@x@\=[1,2,3][:1]@<CR>"#),
        ("x", r#":s@x@\=[1,2,3][1:]@<CR>"#),
        ("x", r#":s@x@\=[1,2,][0]@<CR>"#),
        ("x", r#":s@x@\=1[0]@<CR>"#),
        ("x", r#":s@x@\='abc'[1:0]@<CR>"#),
        ("x", r#":s@x@\=reverse([1])[0]@<CR>"#),
        ("x", r#":s@x@\=[1,2,3][5]@<CR>"#),
        ("x", r#":s@x@\=[1,2][-3]@<CR>"#),
    ]);
}

#[test]
fn built_in_functions() {
    check(&[
        ("x", r#":s@x@\=tolower('AB')@<CR>"#),
        ("x", r#":s@x@\=strlen('hello')@<CR>"#),
        ("x", r#":s@x@\=len([1,2]) . len('ab')@<CR>"#),
        ("x", r#":s@x@\=repeat('ab',3)@<CR>"#),
        ("x", r#":s@x@\=repeat([1],2)@<CR>"#),
        ("x", r#":s@x@\=repeat('a', 0)@<CR>"#),
        ("x", r#":s@x@\=repeat([], 2)@<CR>"#),
        ("x", r#":s@x@\=substitute('a.b','\.','-','g')@<CR>"#),
        ("x", r#":s@x@\=substitute('abab','b','X','')@<CR>"#),
        ("x", r#":s@x@\=substitute('ab','\(a\)','[&\1]','')@<CR>"#),
        (
            "x",
            r#":s@x@\=substitute('ab','\(a\)\(b\)','\2\1','')@<CR>"#,
        ),
        (
            "x",
            r#":s@x@\=printf('%d-%s-%5d|%-4s|%%', 3, 'x', 42, 'ab')@<CR>"#,
        ),
        ("x", r#":s@x@\=printf('%03d', 7)@<CR>"#),
        ("x", r#":s@x@\=printf('%x', 255)@<CR>"#),
        ("x", r#":s@x@\=printf('%5s!', 'ab')@<CR>"#),
        ("x", r#":s@x@\=printf('%-3d!%03d!%X', -2, -2, 255)@<CR>"#),
        ("x", r#":s@x@\=printf('%s', [1])@<CR>"#),
        ("x", r#":s@x@\=printf('%5%!')@<CR>"#),
        ("x", r#":s@x@\=printf('%d', 'x')@<CR>"#),
        ("x", r#":s@x@\=string(1)@<CR>"#),
        ("x", r#":s@x@\=string('a''b')@<CR>"#),
        ("x", r#":s@x@\=string([1,'a'])@<CR>"#),
        ("x", r#":s@x@\=string([])@<CR>"#),
        ("x", r#":s@x@\=string('')@<CR>"#),
        ("x", r#":s@x@\=str2nr('0x1f',16)@<CR>"#),
        ("x", r#":s@x@\=str2nr('ff',16)@<CR>"#),
        ("x", r#":s@x@\=str2nr('12ab')@<CR>"#),
        ("x", r#":s@x@\=str2nr('-5')@<CR>"#),
        ("x", r#":s@x@\=str2nr('x1')@<CR>"#),
        ("x", r#":s@x@\=str2nr('17', 8)@<CR>"#),
        ("x", r#":s@x@\=abs(-3) . abs(4) . abs('x')@<CR>"#),
        ("x", r#":s@x@\=join([1,2],'-')@<CR>"#),
        ("x", r#":s@x@\=join(['a','b'])@<CR>"#),
        ("x", r#":s@x@\=join([])@<CR>"#),
        ("x", r#":s@x@\=split('a b  c')@<CR>"#),
        ("x", r#":s@x@\=split('a,b,,c', ',')@<CR>"#),
        ("x", r#":s@x@\=split('a1b22c', '[0-9]\+')@<CR>"#),
        ("x", r#":s@x@\=split(',a,', ',')@<CR>"#),
        ("x", r#":s@x@\=split(',,a', ',')@<CR>"#),
        ("x", r#":s@x@\=split(',,a', ',', 1)@<CR>"#),
        ("x", r#":s@x@\=split('a,,b', ',')@<CR>"#),
        ("x", r#":s@x@\=split(',', ',')@<CR>"#),
        ("x", r#":s@x@\=split('')@<CR>"#),
        ("x", r#":s@x@\=split('a1b', '\d')@<CR>"#),
        ("x", r#":s@x@\=range(3)@<CR>"#),
        ("x", r#":s@x@\=range(1,6,2)@<CR>"#),
        ("x", r#":s@x@\=range(3,1,-1)@<CR>"#),
        ("x", r#":s@x@\=range(1,2,-1)@<CR>"#),
        ("x", r#":s@x@\=range(0)@<CR>"#),
        ("x", r#":s@x@\=max([1,5,3]) . min([4,2]) . max([])@<CR>"#),
        ("x", r#":s@x@\=reverse([1,2,3])@<CR>"#),
        ("x", r#":s@x@\=reverse('abc')@<CR>"#),
        ("x", r#":s@x@\=trim("\t\n x \r")@<CR>"#),
        ("x", r#":s@x@\=trim("xxaxx", "x")@<CR>"#),
        ("x", r#":s@x@\=strpart('abcdef',2,3)@<CR>"#),
        ("x", r#":s@x@\=strpart('abc',1)@<CR>"#),
        ("x", r#":s@x@\=strpart('abc',5)@<CR>"#),
        ("x", r#":s@x@\=strpart('abc',-1,2)@<CR>"#),
        ("x", r#":s@x@\=strpart('abc',0,-1)@<CR>"#),
        ("x", r#":s@x@\=escape('a.b*c', '.*')@<CR>"#),
        ("x", r#":s@x@\=escape('a/b', '/')@<CR>"#),
        ("x", r#":s@x@\=escape('a\b', '\')@<CR>"#),
        ("x", r#":s@x@\=nr2char(65) . char2nr('A')@<CR>"#),
        ("x", r#":s@x@\=toupper(1) . min([])@<CR>"#),
        ("x", r#":s@x@\=line('.') . col('.') . line('$')@<CR>"#),
    ]);
}

#[test]
fn string_literals() {
    check(&[
        ("x", r#":s@x@\="\t;\x41;\101;\\;\""@<CR>"#),
        ("x", r#":s@x@\="a\<lt>Esc>b"@<CR>"#),
        ("x", r#":s@x@\="a\<lt>CR>b"@<CR>"#),
        ("x", r#":s@x@\='it''s'@<CR>"#),
        ("x", r#":s@x@\="a\qb"@<CR>"#),
        ("x", r#":s@x@\="a\b"@<CR>"#),
        ("x", r#":s@x@\="\u0041\U00000042"@<CR>"#),
        ("x", r#":s@x@\="a"."b" . 'c'@<CR>"#),
    ]);
}

#[test]
fn errors_stop_the_substitute() {
    check(&[
        ("x", r#":s@x@\=foo@<CR>"#),
        ("x", r#":s@x@\=foo()@<CR>"#),
        ("x", r#":s@x@\=strlen()@<CR>"#),
        ("x", r#":s@x@\=strlen(1,2)@<CR>"#),
        ("x", r#":s@x@\=1 +@<CR>"#),
        ("x", r#":s@x@\=(1@<CR>"#),
        ("x", r#":s@x@\=@<CR>"#),
        ("x", r#":s@x@\=submatch(10)@<CR>"#),
        ("x", r#":s@x@\=1 2@<CR>"#),
        ("x", r#":s@x@\=0 ? 1@<CR>"#),
        ("x", r#":s@x@\=[1] + 1@<CR>"#),
        ("x", r#":s@x@\=[1] . 'a'@<CR>"#),
        ("x", r#":s@x@\=range(2,0)@<CR>"#),
        ("x", r#":s@x@\=range(1,2,0)@<CR>"#),
        ("x", r#":s@x@\=join(1)@<CR>"#),
        ("x", r#":s@x@\=split('a', '\(')@<CR>"#),
        ("x", r#":s@x@\=max(1)@<CR>"#),
        ("x", r#":s@x@\=reverse(1)@<CR>"#),
        ("x", r#":s@x@\=printf('%d')@<CR>"#),
        ("x", r#":s@x@\=printf('%d', 1, 2)@<CR>"#),
        ("y", r#":s@x@\=foo@<CR>"#),
        ("xx", r#":s@x@\=(col('.')==1 ? foo : 'y')@g<CR>"#),
        ("xx", r#":s@x@\=(col('.')==2 ? foo : 'y')@g<CR>"#),
        ("xx", r#":s@x@\=(col('.')==2 ? range(2,0) : 'y')@g<CR>"#),
        ("x\nx", r#":%s@x@\=(line('.')==2 ? foo : 'y')@<CR>"#),
        ("x\nx", r#":%s@x@\=(line('.')==2 ? range(2,0) : 'y')@<CR>"#),
        (
            "x\nx\nx",
            r#":%s/x/\=(line('.')==2 ? submatch(10) : 'y')/<CR>"#,
        ),
        ("x\nx\nx", r#":%s@x@\=(line('.')==2 ? foo : 'y')@<CR>u"#),
    ]);
}

#[test]
fn texts_and_flags_combined() {
    let texts = ["x", "x\nxx", "axbx\n\nx"];
    let keys = [
        r#":s@x@\=col('.')@g<CR>"#,
        r#":%s@x@\=line('.') . ':' . submatch(0)@g<CR>"#,
        r#":%s@x\n@\=''@<CR>"#,
        r#":%s@^@\=line('$')@<CR>"#,
    ];
    check(&cross(&texts, &keys));
}

#[test]
fn replacement_with_other_commands() {
    check(&[
        // A `~` in the expression is not the previous replacement.
        ("a", r#":s/a/Q/<CR>:s/Q/\=''~''/<CR>"#),
        ("a", r#":s/a/Q/<CR>:s/Q/~/<CR>"#),
        ("abc", r#":s/a/\="x"/<CR>u"#),
        ("x\nx", r#":%s@x@\=''y''@c<CR>yn"#),
        ("x\nx", r#":s/x/\=line('.')/<CR>j:&<CR>"#),
        ("a\nb\nc", r#":%s/a\nb/\=submatch(0).'X'/<CR>"#),
    ]);
}
