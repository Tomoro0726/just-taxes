//! メイン画面の窓口。その日の来客を順に迎え、机の上の書類をプレイヤーが審査して判定する。
//!
//! 来客ごとに「入場」の会話 → 判定のボタン → 判定に応じた会話、と進む。
//! 正解は書類の値から決まる (偽造・項目の食い違いなら強制退去、書類が足りなければ不足を指摘)。

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::books::PointerBlocked;
use crate::dialogue::{Typewriter, finish_scene};
use crate::npc::{
    ASK_FOUND, ASK_LOST, Document, ENTER, EVICT_FORGED, EVICT_MISMATCH, LEAVE,
};
use crate::panel::{DotFont, DotRect, DotStyle, StageNode, dot_node, rect, stage_transform};
use crate::scenario::{Dialogue, Story};
use crate::transition::Transition;
use crate::UiFont;

/// 書類どうしで一致していなければならない項目
const MATCHING_FIELDS: [&str; 3] = ["許可番号", "名前", "屋号"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verdict {
    Pass,
    Missing,
    Evict,
}

/// 判定のボタンを並べる、左下の会話画面の下端。判定中は会話画面をこの分だけ縮める
pub const BUTTON_BAR: DotRect = rect(2.0, 250.0, 124.0, 18.0);

const BUTTONS: [(Verdict, &str, DotRect); 3] = [
    (Verdict::Pass, "通す", rect(4.0, 252.0, 38.0, 14.0)),
    (Verdict::Missing, "不足を指摘", rect(45.0, 252.0, 38.0, 14.0)),
    (Verdict::Evict, "強制退去", rect(86.0, 252.0, 38.0, 14.0)),
];

/// 会話画面の背景と同じ色
const BAR_FILL: Color = Color::srgb_u8(0x4d, 0x2b, 0x32);
const BUTTON_FILL: Color = Color::srgb_u8(0xe8, 0xd4, 0xb0);
const BUTTON_HOVER: Color = Color::srgb_u8(0xf8, 0xec, 0xd0);
const BUTTON_EDGE: Color = Color::srgb_u8(0x6e, 0x48, 0x34);
const BUTTON_INK: Color = Color::srgb_u8(0x2a, 0x22, 0x32);

#[derive(Debug, PartialEq)]
enum Problem {
    Forged,
    Mismatch(&'static str),
    Missing(String),
}

impl Problem {
    fn verdict(problem: Option<&Self>) -> Verdict {
        match problem {
            None => Verdict::Pass,
            Some(Self::Missing(_)) => Verdict::Missing,
            Some(Self::Forged | Self::Mismatch(_)) => Verdict::Evict,
        }
    }
}

/// 机の上の書類の問題。偽造と食い違いは、書類が足りなくても見つけた時点で強制退去にできる
fn inspect(desk: &[Document], required: &[String]) -> Option<Problem> {
    if desk.iter().any(|d| d.forged) {
        return Some(Problem::Forged);
    }
    for field in MATCHING_FIELDS {
        let mut values = desk.iter().filter_map(|d| d.fields.get(field));
        if let Some(first) = values.next()
            && values.any(|v| v != first)
        {
            return Some(Problem::Mismatch(field));
        }
    }
    required
        .iter()
        .find(|kind| !desk.iter().any(|d| &d.kind == *kind))
        .map(|kind| Problem::Missing(kind.clone()))
}

/// 今流している会話が終わったら何をするか
#[derive(Clone, Copy, Default, PartialEq)]
enum Then {
    #[default]
    Welcome,
    Judge,
    FinishScene,
}

#[derive(Resource, Default)]
pub struct Counter {
    /// この状態がどのシーンのものか
    scene: Option<usize>,
    /// 次に迎える来客の番号 (今の来客は 1 つ前)
    pub next: usize,
    then: Then,
    pub judging: bool,
    hover: Option<usize>,
    judged: u32,
    correct: u32,
    /// 机の上に出ている今の来客の書類
    pub desk: Vec<Document>,
}

/// 会話を足して、その先頭から流す
fn say(story: &mut Story, typewriter: &mut Typewriter, lines: Vec<Dialogue>) {
    let dialogues = &mut story.scene_mut().dialogues;
    let start = dialogues.len();
    dialogues.extend(lines);
    if dialogues.len() > start {
        story.dialogue_idx = start;
        typewriter.reset();
    }
}

fn narration(text: String) -> Dialogue {
    Dialogue {
        speaker: None,
        text,
        speed: None,
    }
}

/// 会話の `[書類]` と `[許可番号/名前/屋号]` を、判定の理由で置き換える
fn fill(lines: &[Dialogue], document: &str, field: &str) -> Vec<Dialogue> {
    lines
        .iter()
        .map(|d| Dialogue {
            text: d
                .text
                .replace("[書類]", document)
                .replace("[許可番号/名前/屋号]", field),
            ..d.clone()
        })
        .collect()
}

/// 会話が尽きたら、来客を迎える・判定を待つ・シーンを終える
pub fn run_counter(
    mut story: ResMut<Story>,
    mut counter: ResMut<Counter>,
    mut transition: ResMut<Transition>,
    mut typewriter: ResMut<Typewriter>,
) {
    if counter.scene != Some(story.scene_idx) {
        *counter = Counter {
            scene: Some(story.scene_idx),
            ..default()
        };
        // 前置きの会話がなければ、すぐ最初の来客を迎える
        story.waiting = story.scene().day.is_some() && story.scene().dialogues.is_empty();
    }
    if !story.waiting {
        return;
    }
    let Some(day) = story.scene().day.clone() else {
        return;
    };
    story.waiting = false;

    match counter.then {
        Then::Judge => counter.judging = true,
        Then::FinishScene => finish_scene(&mut story, &mut transition),
        Then::Welcome => {
            if let Some(npc) = day.visitors.get(counter.next) {
                counter.next += 1;
                counter.desk = npc.documents.clone();
                counter.then = Then::Judge;
                say(&mut story, &mut typewriter, npc.conversation(ENTER).to_vec());
            } else {
                counter.desk.clear();
                counter.then = Then::FinishScene;
                let lines = vec![
                    narration("（本日の窓口はここまで）".into()),
                    narration(format!(
                        "（判定 {} 回のうち、正しかったのは {} 回）",
                        counter.judged, counter.correct
                    )),
                ];
                say(&mut story, &mut typewriter, lines);
            }
        }
    }
}

/// 判定のボタンを押したら、正誤を記録して判定に応じた会話を流す
#[allow(clippy::too_many_arguments)]
pub fn counter_input(
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    transition: Res<Transition>,
    mut story: ResMut<Story>,
    mut counter: ResMut<Counter>,
    mut typewriter: ResMut<Typewriter>,
    mut blocked: ResMut<PointerBlocked>,
) {
    counter.hover = None;
    if !counter.judging || transition.active() {
        return;
    }
    let (scale, origin) = stage_transform(&window);
    let Some(cursor) = window.cursor_position().map(|c| (c - origin) / scale) else {
        return;
    };
    counter.hover = BUTTONS.iter().position(|(_, _, r)| {
        Rect::new(r.x, r.y, r.x + r.w, r.y + r.h).contains(cursor)
    });
    blocked.0 |= counter.hover.is_some();
    let (Some(button), true) = (counter.hover, mouse.just_pressed(MouseButton::Left)) else {
        return;
    };
    let Some(day) = story.scene().day.clone() else {
        return;
    };
    let Some(npc) = counter.next.checked_sub(1).and_then(|i| day.visitors.get(i)) else {
        return;
    };

    let verdict = BUTTONS[button].0;
    let problem = inspect(&counter.desk, &day.required);
    counter.judging = false;
    counter.judged += 1;
    counter.correct += u32::from(verdict == Problem::verdict(problem.as_ref()));

    let field = match problem {
        Some(Problem::Mismatch(field)) => field,
        _ => "記載内容",
    };
    let (lines, then) = match (verdict, &problem) {
        (Verdict::Pass, _) => (fill(npc.conversation(LEAVE), "", field), Then::Welcome),
        (Verdict::Evict, _) => {
            let mut lines = npc.conversation(EVICT_MISMATCH);
            if problem == Some(Problem::Forged) && !npc.conversation(EVICT_FORGED).is_empty() {
                lines = npc.conversation(EVICT_FORGED);
            }
            (fill(lines, "", field), Then::Welcome)
        }
        (Verdict::Missing, Some(Problem::Missing(kind))) => {
            if let Some(doc) = npc.extra.iter().find(|d| &d.kind == kind) {
                counter.desk.push(doc.clone());
                (fill(npc.conversation(ASK_FOUND), kind, field), Then::Judge)
            } else {
                (fill(npc.conversation(ASK_LOST), kind, field), Then::Welcome)
            }
        }
        (Verdict::Missing, _) => (
            vec![narration("（必要な書類はすべてそろっている）".into())],
            Then::Judge,
        ),
    };
    counter.then = then;
    say(&mut story, &mut typewriter, lines);
}

// ─── 判定のボタン ────────────────────────────────────────────

#[derive(Component)]
pub struct ButtonsRoot;

#[derive(Component)]
pub struct VerdictButton(usize);

pub fn spawn_buttons(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            ButtonsRoot,
            StageNode,
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            // 巻物より手前、トランジションより下
            GlobalZIndex(40),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            // 縮めた会話画面の下に、背景に描かれた会話欄が見えないようにする
            root.spawn((dot_node(BUTTON_BAR), BackgroundColor(BAR_FILL)));
            for (i, (_, label, r)) in BUTTONS.into_iter().enumerate() {
                root.spawn((
                    VerdictButton(i),
                    Node {
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..dot_node(r)
                    },
                    DotStyle {
                        border: 1.0,
                        radius: 2.0,
                        ..default()
                    },
                    BackgroundColor(BUTTON_FILL),
                    BorderColor::all(BUTTON_EDGE),
                ))
                .with_child((
                    Text::new(label),
                    TextFont::from(font.0.clone()),
                    DotFont(6.0),
                    TextColor(BUTTON_INK),
                ));
            }
        });
}

pub fn draw_buttons(
    counter: Res<Counter>,
    transition: Res<Transition>,
    mut root: Single<&mut Visibility, With<ButtonsRoot>>,
    mut buttons: Query<(&VerdictButton, &mut BackgroundColor)>,
) {
    **root = if counter.judging && !transition.active() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for (button, mut fill) in &mut buttons {
        fill.0 = if counter.hover == Some(button.0) {
            BUTTON_HOVER
        } else {
            BUTTON_FILL
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(kind: &str, fields: &[(&str, &str)], forged: bool) -> Document {
        Document {
            kind: kind.into(),
            fields: fields.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
            forged,
            blocks: Vec::new(),
        }
    }

    #[test]
    fn finds_problems() {
        let required = ["販売許可証".to_owned(), "納税申告書".to_owned()];
        let permit = doc("販売許可証", &[("屋号", "海運")], false);
        let ok = doc("納税申告書", &[("屋号", "海運")], false);
        let other = doc("納税申告書", &[("屋号", "商船")], false);
        let forged = doc("販売許可証", &[("屋号", "海運")], true);

        assert_eq!(inspect(&[permit.clone(), ok.clone()], &required), None);
        assert_eq!(
            inspect(std::slice::from_ref(&permit), &required),
            Some(Problem::Missing("納税申告書".into()))
        );
        assert_eq!(
            inspect(&[permit, other], &required),
            Some(Problem::Mismatch("屋号"))
        );
        assert_eq!(inspect(&[forged, ok], &required), Some(Problem::Forged));
    }
}
