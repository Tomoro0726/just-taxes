//! 巻物の中身を書く Markdown のサブセット。
//!
//! - `# 見出し`
//! - 段落 (空行で区切る。段落内の改行はそのまま改行として表示する)
//! - `---` 区切り線
//! - `| 項目 | 値 | 値 |` 表の行。1 列目は左寄せ、2 列目以降は右寄せで縦にそろう。
//!   `|---|--:|` の区切り行より上の行は見出し行として薄い色で表示する。
//!   区切り行は省略でき、表の途中に `---` を挟んで合計行を分けられる
//! - `> ` 引用 → 枠で囲んだカード (中身にも上の書式が使える)

/// 先頭の `---` の行で囲んだ YAML (フロントマター) と、残りの本文に分ける
pub fn split_front_matter(md: &str) -> (Option<&str>, &str) {
    let md = md.trim_start_matches('\u{feff}');
    let mut lines = md.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return (None, md);
    }
    let start = md.find('\n').map_or(md.len(), |i| i + 1);
    let mut end = start;
    for line in lines {
        if line.trim_end() == "---" {
            return (Some(&md[start..end]), &md[end + line.len()..]);
        }
        end += line.len();
    }
    (None, md)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(String),
    Text(String),
    Rule,
    Row { cells: Vec<String>, head: bool },
    Card(Vec<Block>),
}

pub fn parse(md: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut para = Vec::new();
    let mut quote = Vec::new();
    // 今の表の最初の行の位置 (区切り行より上を見出し行にする)
    let mut table_start = None;

    for line in md.lines().map(str::trim) {
        if let Some(q) = line.strip_prefix('>') {
            flush(&mut blocks, &mut para, &mut Vec::new());
            quote.push(q.trim_start());
            continue;
        }
        flush(&mut blocks, &mut Vec::new(), &mut quote);

        if let Some(row) = line.strip_prefix('|') {
            flush(&mut blocks, &mut para, &mut quote);
            let row = row.strip_suffix('|').unwrap_or(row);
            let cells: Vec<String> = row.split('|').map(|c| c.trim().to_owned()).collect();
            let start = *table_start.get_or_insert(blocks.len());
            let is_separator = cells
                .iter()
                .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')));
            if is_separator {
                for block in &mut blocks[start..] {
                    if let Block::Row { head, .. } = block {
                        *head = true;
                    }
                }
            } else {
                blocks.push(Block::Row { cells, head: false });
            }
            continue;
        }
        table_start = None;

        if line.is_empty() {
            flush(&mut blocks, &mut para, &mut quote);
        } else if line.starts_with('#') {
            flush(&mut blocks, &mut para, &mut quote);
            blocks.push(Block::Heading(line.trim_start_matches('#').trim().to_owned()));
        } else if line.len() >= 3 && line.chars().all(|c| c == '-') {
            flush(&mut blocks, &mut para, &mut quote);
            blocks.push(Block::Rule);
        } else {
            para.push(line);
        }
    }
    flush(&mut blocks, &mut para, &mut quote);
    blocks
}

fn flush(blocks: &mut Vec<Block>, para: &mut Vec<&str>, quote: &mut Vec<&str>) {
    if !para.is_empty() {
        blocks.push(Block::Text(para.join("\n")));
        para.clear();
    }
    if !quote.is_empty() {
        blocks.push(Block::Card(parse(&quote.join("\n"))));
        quote.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(cells: &[&str], head: bool) -> Block {
        Block::Row {
            cells: cells.iter().map(|&c| c.to_owned()).collect(),
            head,
        }
    }

    #[test]
    fn parses_blocks() {
        let md = "# 元帳\n\n一行目\n二行目\n\n---\n本文";
        assert_eq!(
            parse(md),
            [
                Block::Heading("元帳".into()),
                Block::Text("一行目\n二行目".into()),
                Block::Rule,
                Block::Text("本文".into()),
            ]
        );
    }

    #[test]
    fn rows_above_separator_are_head() {
        let md = "| | 前期 | 今期 |\n|---|--:|--:|\n| 水利 | 1 | 2 |\n---\n| 計 | | 3 |";
        assert_eq!(
            parse(md),
            [
                row(&["", "前期", "今期"], true),
                row(&["水利", "1", "2"], false),
                Block::Rule,
                row(&["計", "", "3"], false),
            ]
        );
    }

    #[test]
    fn splits_front_matter() {
        assert_eq!(
            split_front_matter("---\na: 1\n---\n本文\n"),
            (Some("a: 1\n"), "本文\n")
        );
        assert_eq!(split_front_matter("本文\n---\n"), (None, "本文\n---\n"));
        assert_eq!(split_front_matter("---\n閉じない\n"), (None, "---\n閉じない\n"));
    }

    #[test]
    fn quote_becomes_card() {
        let md = "> 許可証\n>\n> | 番号 | 1A |\n後";
        assert_eq!(
            parse(md),
            [
                Block::Card(vec![Block::Text("許可証".into()), row(&["番号", "1A"], false)]),
                Block::Text("後".into()),
            ]
        );
    }
}
