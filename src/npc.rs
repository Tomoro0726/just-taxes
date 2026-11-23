//! 窓口に来る NPC (`assets/npcs/<id>.md`)、その持ち物の書類 (`assets/documents/*.md`)、
//! 1日の来客 (`assets/days/<id>.yaml`) の読み込み。
//!
//! NPC のファイルは、先頭の YAML に名前と持ち物 (書類のファイル) を書き、
//! 本文に `## 入場` のような見出しごとの会話を `PC: …` / `NPC: …` の行で書く。
//! 書類のファイルは、先頭の YAML に判定用の値を書き、本文は巻物と同じ Markdown で書く。
//! 本文の `[項目]` は先頭の YAML の値に置き換える。

use std::collections::BTreeMap;
use std::hash::{BuildHasher, RandomState};
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use crate::markdown::{self, Block, split_front_matter};
use crate::scenario::{Dialogue, value_to_string};

pub const ENTER: &str = "入場";
pub const LEAVE: &str = "退場";
pub const ASK_FOUND: &str = "問答 提出";
pub const ASK_LOST: &str = "問答 紛失";
pub const EVICT_MISMATCH: &str = "強制退去 不一致";
/// 書かれていなければ「強制退去 不一致」の会話を使う
pub const EVICT_FORGED: &str = "強制退去 偽造";

#[derive(Clone, Debug)]
pub struct Document {
    /// 書類の種類 (先頭の YAML の `書類`)
    pub kind: String,
    /// 先頭の YAML の値 (許可番号・名前・屋号など)
    pub fields: BTreeMap<String, String>,
    pub forged: bool,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug)]
pub struct Npc {
    /// 見出しと会話。同じ見出しが複数あれば、そのうち 1 つを使う
    sections: Vec<(String, Vec<Dialogue>)>,
    /// 入場したときに出す書類
    pub documents: Vec<Document>,
    /// 不足を指摘されたら出す書類
    pub extra: Vec<Document>,
}

#[derive(Clone, Debug)]
pub struct Day {
    /// 全員がそろえるべき書類の種類
    pub required: Vec<String>,
    pub visitors: Vec<Npc>,
}

#[derive(Deserialize)]
struct NpcMeta {
    #[serde(rename = "名前")]
    name: String,
    #[serde(rename = "持ち物", default)]
    items: Vec<String>,
    #[serde(rename = "追加の持ち物", default)]
    extra: Vec<String>,
}

#[derive(Deserialize)]
struct DayMeta {
    #[serde(rename = "必要な書類")]
    required: Vec<String>,
    #[serde(rename = "来客")]
    visitors: Vec<String>,
}

impl Day {
    /// `player` は `PC:` の行の話者名にする
    pub fn load(assets: &Path, id: &str, player: Option<&str>) -> Result<Arc<Self>, String> {
        let path = assets.join("days").join(format!("{id}.yaml"));
        let meta: DayMeta = serde_yaml::from_str(&read(&path)?)
            .map_err(|e| format!("{} の解析に失敗: {e}", path.display()))?;
        let visitors = meta
            .visitors
            .iter()
            .map(|npc| Npc::load(assets, npc, player))
            .collect::<Result<_, _>>()?;
        Ok(Arc::new(Self {
            required: meta.required,
            visitors,
        }))
    }
}

impl Npc {
    fn load(assets: &Path, id: &str, player: Option<&str>) -> Result<Self, String> {
        let path = assets.join("npcs").join(format!("{id}.md"));
        let md = read(&path)?;
        let (meta, body) = split_front_matter(&md);
        let meta: NpcMeta = serde_yaml::from_str(meta.unwrap_or_default())
            .map_err(|e| format!("{} の先頭の YAML を読めません: {e}", path.display()))?;

        let documents = |items: &[String]| {
            items
                .iter()
                .map(|item| load_document(&assets.join("documents").join(item)))
                .collect::<Result<Vec<_>, _>>()
        };

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

        let npc = Self {
            sections,
            documents: documents(&meta.items)?,
            extra: documents(&meta.extra)?,
        };
        let mut needed = vec![ENTER, LEAVE, ASK_LOST, EVICT_MISMATCH];
        if !npc.extra.is_empty() {
            needed.push(ASK_FOUND);
        }
        if let Some(missing) = needed.iter().find(|h| npc.conversation(h).is_empty()) {
            return Err(format!("{}: 「## {missing}」の会話がありません", path.display()));
        }
        Ok(npc)
    }

    /// 見出し `heading` の会話。同じ見出しが複数あればランダムに 1 つ選び、なければ空
    pub fn conversation(&self, heading: &str) -> &[Dialogue] {
        let found: Vec<&[Dialogue]> = self
            .sections
            .iter()
            .filter(|(h, _)| h == heading)
            .map(|(_, d)| d.as_slice())
            .collect();
        if found.is_empty() {
            return &[];
        }
        found[RandomState::new().hash_one(()) as usize % found.len()]
    }
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{} を読めません ({e})", path.display()))
}

fn load_document(path: &Path) -> Result<Document, String> {
    let md = read(path)?;
    let (meta, body) = split_front_matter(&md);
    let mut fields: BTreeMap<String, String> = match meta {
        Some(meta) if !meta.trim().is_empty() => {
            serde_yaml::from_str::<BTreeMap<String, serde_yaml::Value>>(meta)
                .map_err(|e| format!("{} の先頭の YAML を読めません: {e}", path.display()))?
                .iter()
                .map(|(k, v)| (k.clone(), value_to_string(v)))
                .collect()
        }
        _ => BTreeMap::new(),
    };
    let body = fields.iter().fold(body.to_owned(), |body, (key, value)| {
        body.replace(&format!("[{key}]"), value)
    });
    Ok(Document {
        kind: fields.remove("書類").unwrap_or_default(),
        forged: fields.remove("偽造").is_some_and(|v| v == "true"),
        fields,
        blocks: markdown::parse(&body),
    })
}
