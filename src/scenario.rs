//! シナリオ (YAML) の読み込みと進行状態。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;
use serde::Deserialize;

use crate::npc::Day;

// ─── トランジション ──────────────────────────────────────────

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransitionType {
    #[default]
    Fade,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    None,
}

// ─── 内部実行用の型 ──────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Dialogue {
    pub speaker: Option<String>,
    pub text: String,
    /// 1文字あたりの表示間隔 (ms)
    pub speed: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct Next {
    pub scene: String,
    pub transition_out: Option<TransitionType>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub id: String,
    pub background: String,
    pub transition_in: Option<TransitionType>,
    pub dialogues: Vec<Dialogue>,
    pub next: Option<Next>,
    /// 窓口のシーンなら、その日の来客。シーンの会話のあとに来客を順に迎える (counter.rs)
    pub day: Option<Arc<Day>>,
}

#[derive(Clone, Debug)]
pub struct Chapter {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub title: String,
    /// 主人公 (チャット表示で右側に出す話者)
    pub player: Option<String>,
    pub scenes: Vec<Scene>,
}

// ─── YAMLからの生データ型 ────────────────────────────────────
// yamlの `dialogues` は { "話者": "セリフ" } または { text: "セリフ" } のリスト

type YamlDialogue = BTreeMap<String, serde_yaml::Value>;

#[derive(Deserialize)]
#[serde(untagged)]
enum YamlNext {
    // next: "scene2" という省略記法も許容
    Short(String),
    Full {
        scene: String,
        transition_out: Option<TransitionType>,
    },
}

#[derive(Deserialize)]
struct YamlScene {
    id: String,
    background: String,
    transition_in: Option<TransitionType>,
    /// 1日の来客 (`assets/days/<id>.yaml`)
    day: Option<String>,
    #[serde(default)]
    dialogues: Vec<YamlDialogue>,
    next: Option<YamlNext>,
}

#[derive(Deserialize)]
struct YamlChapter {
    id: String,
    title: String,
    player: Option<String>,
    scenes: Vec<YamlScene>,
}

#[derive(Deserialize)]
struct ScenarioIndex {
    scenarios: Vec<ScenarioEntry>,
}

#[derive(Deserialize)]
struct ScenarioEntry {
    id: String,
}

pub(crate) fn value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim_end()
            .to_string(),
    }
}

fn normalize_dialogue(d: &YamlDialogue) -> Dialogue {
    let mut keys = d.keys();
    if let (Some(key), None) = (keys.next(), keys.next())
        && key != "text"
        && key != "speed"
    {
        // 例: { "アル": "こんにちは" }
        // narrator はナレーション扱いで話者名を出さない
        let speaker = (key != "narrator").then(|| key.clone());
        return Dialogue {
            speaker,
            text: value_to_string(&d[key]),
            speed: None,
        };
    }
    // 例: { text: "ナレーション", speaker: "アル", speed: 80 }
    Dialogue {
        speaker: d.get("speaker").map(value_to_string),
        text: d.get("text").map(value_to_string).unwrap_or_default(),
        speed: d.get("speed").and_then(|v| v.as_f64()).map(|v| v as f32),
    }
}

fn normalize_next(n: YamlNext) -> Next {
    match n {
        YamlNext::Short(scene) => Next {
            scene,
            transition_out: None,
        },
        YamlNext::Full {
            scene,
            transition_out,
        } => Next {
            scene,
            transition_out,
        },
    }
}

fn normalize_scene(s: YamlScene, assets: &Path, player: Option<&str>) -> Result<Scene, String> {
    Ok(Scene {
        day: s
            .day
            .as_deref()
            .map(|id| Day::load(assets, id, player))
            .transpose()?,
        id: s.id,
        background: s.background,
        transition_in: s.transition_in,
        dialogues: s.dialogues.iter().map(normalize_dialogue).collect(),
        next: s.next.map(normalize_next),
    })
}

fn normalize(y: YamlChapter, assets: &Path) -> Result<Chapter, String> {
    let scenes = y
        .scenes
        .into_iter()
        .map(|s| normalize_scene(s, assets, y.player.as_deref()))
        .collect::<Result<_, _>>()?;
    Ok(Chapter {
        id: y.id,
        title: y.title,
        player: y.player,
        scenes,
    })
}

// ─── 読み込み ────────────────────────────────────────────────

fn read_yaml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("シナリオが見つかりません: {} ({e})", path.display()))?;
    serde_yaml::from_str(&text).map_err(|e| format!("{} の解析に失敗: {e}", path.display()))
}

/// 起動引数 "chapterId[/sceneId[/dialogueIdx]]" で指定されたシナリオを読み込む。
/// 省略時は index.yaml の先頭シナリオ。
pub fn load_story(arg: Option<&str>) -> Result<Story, String> {
    let assets = bevy::asset::io::file::FileAssetReader::get_base_path().join("assets");
    let dir = assets.join("scenarios");

    let arg = arg.unwrap_or_default();
    let mut parts = arg.split('/');
    let mut chapter_id = parts.next().unwrap_or_default().to_string();
    let target_scene = parts.next().filter(|s| !s.is_empty());
    let target_dialogue = parts.next().and_then(|s| s.parse::<usize>().ok());

    if chapter_id.is_empty() {
        let index: ScenarioIndex = read_yaml(&dir.join("index.yaml"))?;
        chapter_id = index
            .scenarios
            .first()
            .ok_or("index.yaml にシナリオがありません")?
            .id
            .clone();
    }

    let chapter = normalize(
        read_yaml(&dir.join(format!("{chapter_id}.yaml")))?,
        &assets,
    )?;
    if chapter.scenes.is_empty() {
        return Err("シーンデータが空です".into());
    }

    // シーンが指定されていれば、デバッグ用にそこへ直接ジャンプする
    let scene_idx = target_scene
        .and_then(|id| chapter.scenes.iter().position(|s| s.id == id))
        .unwrap_or(0);
    let dialogue_idx = target_dialogue
        .map(|i| i.min(chapter.scenes[scene_idx].dialogues.len().saturating_sub(1)))
        .unwrap_or(0);

    Ok(Story {
        chapter,
        scene_idx,
        dialogue_idx,
        ended: false,
        waiting: false,
    })
}

// ─── 進行状態 ────────────────────────────────────────────────

#[derive(Resource)]
pub struct Story {
    pub chapter: Chapter,
    pub scene_idx: usize,
    pub dialogue_idx: usize,
    pub ended: bool,
    /// 窓口のシーンで会話が尽き、次に何をするかを counter.rs が決めるのを待っている
    pub waiting: bool,
}

impl Story {
    pub fn scene(&self) -> &Scene {
        &self.chapter.scenes[self.scene_idx]
    }

    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.chapter.scenes[self.scene_idx]
    }

    pub fn dialogue(&self) -> Option<&Dialogue> {
        self.scene().dialogues.get(self.dialogue_idx)
    }

    pub fn find_scene(&self, id: &str) -> Option<usize> {
        self.chapter.scenes.iter().position(|s| s.id == id)
    }
}

/// 読み込み失敗時のエラーメッセージ
#[derive(Resource)]
pub struct LoadError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_counter_scene() {
        let story = load_story(Some("chapter1/counter")).unwrap();
        let day = story.scene().day.as_ref().unwrap();
        assert!(!day.visitors.is_empty());
    }
}
