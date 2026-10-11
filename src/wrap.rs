//! 等幅フォント (DotGothic16) 前提の日本語の折り返し。
//!
//! Bevy のテキストレイアウトは日本語の単語区切りを持たず幅で折り返せないので、
//! 全角 = 1em・半角 = 0.5em として自前で改行位置を決める。

use bevy::prelude::Component;

/// この幅 (em) で自前で折り返して表示する
#[derive(Component, Clone, Copy)]
pub struct WrapEm(pub f32);

/// 行頭に来てはいけない文字 (前の行の末尾にぶら下げる)
const NO_LINE_START: &str = "、。，．,.・：；:;？！?!ー～…‥)）]］}｝」』】〕〉》ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ";

fn char_em(c: char) -> f32 {
    // ASCII と半角カナは半分の幅
    if c.is_ascii() || ('\u{ff61}'..='\u{ff9f}').contains(&c) {
        0.5
    } else {
        1.0
    }
}

/// 各行の範囲 (元の文字列のバイト位置) を返す。元の改行は行の区切りとして扱い、範囲に含めない
pub fn wrap_lines(text: &str, max_em: f32) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut width = 0.0;

    for (i, c) in text.char_indices() {
        if c == '\n' {
            lines.push((start, i));
            start = i + c.len_utf8();
            width = 0.0;
            continue;
        }
        let w = char_em(c);
        if width + w > max_em && i > start && !NO_LINE_START.contains(c) {
            lines.push((start, i));
            start = i;
            width = 0.0;
        }
        width += w;
    }
    lines.push((start, text.len()));
    lines
}

/// 折り返した全文
pub fn wrap(text: &str, max_em: f32) -> String {
    let lines: Vec<&str> = wrap_lines(text, max_em)
        .into_iter()
        .map(|(a, b)| &text[a..b])
        .collect();
    lines.join("\n")
}

/// 折り返した全文を `split` (元の文字列のバイト位置) で「表示済み」と「未表示」に分ける。
/// 行の区切りは、その次の行が 1 文字でも表示されていれば表示済み側に入れる
pub fn wrap_split(text: &str, split: usize, max_em: f32) -> (String, String) {
    let mut shown = String::new();
    let mut rest = String::new();
    for (n, (a, b)) in wrap_lines(text, max_em).into_iter().enumerate() {
        if n > 0 {
            if split > a { &mut shown } else { &mut rest }.push('\n');
        }
        let mid = split.clamp(a, b);
        shown.push_str(&text[a..mid]);
        rest.push_str(&text[mid..b]);
    }
    (shown, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_by_width() {
        assert_eq!(wrap("あいうえおかきく", 3.0), "あいう\nえおか\nきく");
        // 半角は 0.5em
        assert_eq!(wrap("abcdefg", 2.0), "abcd\nefg");
    }

    #[test]
    fn keeps_punctuation_off_line_start() {
        // 「。」は行頭に置かず前の行にぶら下げる
        assert_eq!(wrap("あいう。えお", 3.0), "あいう。\nえお");
        assert_eq!(wrap("あいっう", 2.0), "あいっ\nう");
    }

    #[test]
    fn keeps_original_newlines() {
        assert_eq!(wrap("あい\nうえお", 2.0), "あい\nうえ\nお");
    }

    #[test]
    fn split_keeps_layout() {
        let text = "あいうえお";
        for i in 0..=text.chars().count() {
            let split = text.char_indices().nth(i).map_or(text.len(), |(b, _)| b);
            let (shown, rest) = wrap_split(text, split, 2.0);
            assert_eq!(format!("{shown}{rest}"), "あい\nうえ\nお");
        }
        let (shown, rest) = wrap_split(text, "あい".len(), 2.0);
        assert_eq!((shown.as_str(), rest.as_str()), ("あい", "\nうえ\nお"));
        let (shown, rest) = wrap_split(text, "あいう".len(), 2.0);
        assert_eq!((shown.as_str(), rest.as_str()), ("あい\nう", "え\nお"));
    }
}
