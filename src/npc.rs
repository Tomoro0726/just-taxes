//! 窓口に来る NPC (`assets/npcs/<id>.md`) と、その持ち物の書類 (`assets/documents/*.md`)。
//!
//! NPC のファイルは、先頭の YAML に名前と持ち物 (書類のファイル) を書き、
//! 本文に `## 入場` のような見出しごとの会話を `PC: …` / `NPC: …` の行で書く。
//! 書類のファイルは、先頭の YAML に判定用の値を書き、本文は巻物と同じ Markdown で書く。
//! 本文の `[項目]` は先頭の YAML の値に置き換える。

use std::collections::BTreeMap;
use std::hash::{BuildHasher, RandomState};
use std::path::Path;

use serde::Deserialize;

use crate::markdown::{self, Block, split_front_matter};
use crate::scenario::{Dialogue, value_to_string};

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "名前")]
    name: String,
    #[serde(rename = "持ち物", default)]
    items: Vec<String>,
}

pub struct Npc {
    /// 見出しと会話。同じ見出しが複数あれば、そのうち 1 つを使う
    sections: Vec<(String, Vec<Dialogue>)>,
    /// 持ち物の書類の中身
    pub documents: Vec<Vec<Block>>,
}

impl Npc {
    /// `player` は `PC:` の行の話者名にする
    pub fn load(assets: &Path, id: &str, player: Option<&str>) -> Result<Self, String> {
        let path = assets.join("npcs").join(format!("{id}.md"));
        let md = read(&path)?;
        let (meta, body) = split_front_matter(&md);
        let meta: Meta = serde_yaml::from_str(meta.unwrap_or_default())
            .map_err(|e| format!("{} の先頭の YAML を読めません: {e}", path.display()))?;

        let documents = meta
            .items
            .iter()
            .map(|item| load_document(&assets.join("documents").join(item)))
            .collect::<Result<_, _>>()?;

        let mut sections: Vec<(String, Vec<Dialogue>)> = Vec::new();
        for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if line.starts_with('#') {
                let heading = line.trim_start_matches('#').trim().to_owned();
                sections.push((heading, Vec::new()));
                continue;
            }
            let Some((_, dialogues)) = sections.last_mut() else {
                return Err(format!("{}: 見出しより前に会話があります", path.display()));
            };
            let (speaker, text) = match line.split_once([':', '：']) {
                Some(("PC", text)) => (Some(player.unwrap_or("PC").to_owned()), text),
                Some(("NPC", text)) => (Some(meta.name.clone()), text),
                // 話者のない行はナレーション
                _ => (None, line),
            };
            dialogues.push(Dialogue {
                speaker,
                text: text.trim().to_owned(),
                speed: None,
            });
        }

        Ok(Self {
            sections,
            documents,
        })
    }

    /// 見出し `heading` の会話。同じ見出しが複数あればランダムに 1 つ選ぶ
    pub fn conversation(&self, heading: &str) -> Option<&[Dialogue]> {
        let found: Vec<&[Dialogue]> = self
            .sections
            .iter()
            .filter(|(h, _)| h == heading)
            .map(|(_, d)| d.as_slice())
            .collect();
        if found.is_empty() {
            return None;
        }
        let i = RandomState::new().hash_one(()) as usize % found.len();
        Some(found[i])
    }
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{} を読めません ({e})", path.display()))
}

fn load_document(path: &Path) -> Result<Vec<Block>, String> {
    let md = read(path)?;
    let (meta, body) = split_front_matter(&md);
    let fields: BTreeMap<String, serde_yaml::Value> = match meta {
        Some(meta) if !meta.trim().is_empty() => serde_yaml::from_str(meta)
            .map_err(|e| format!("{} の先頭の YAML を読めません: {e}", path.display()))?,
        _ => BTreeMap::new(),
    };
    let body = fields.iter().fold(body.to_owned(), |body, (key, value)| {
        body.replace(&format!("[{key}]"), &value_to_string(value))
    });
    Ok(markdown::parse(&body))
}
