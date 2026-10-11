//! セリフウィンドウ (タイプライター表示・話者名バッジ・送り操作)。
//! 背景に会話欄が描かれているシーンでは、画面下のウィンドウの代わりにそちらへ表示する (panel.rs)。

use bevy::prelude::*;

use crate::UiFont;
use crate::books::PointerBlocked;
use crate::panel::panel_for;
use crate::scenario::Story;
use crate::transition::Transition;
use crate::wrap::{WrapEm, wrap_split};

/// デフォルト表示速度: ms/文字
const DEFAULT_SPEED: f32 = 45.0;

const FRAME_LIGHT: Color = Color::srgb_u8(0xc8, 0xc0, 0xe8);
const FRAME_INNER: Color = Color::srgb_u8(0x68, 0x58, 0xc0);
const BOX_BG: Color = Color::srgba(4.0 / 255.0, 4.0 / 255.0, 18.0 / 255.0, 0.92);
const BADGE_BG: Color = Color::srgba(4.0 / 255.0, 4.0 / 255.0, 18.0 / 255.0, 0.95);
const TEXT_COLOR: Color = Color::srgb_u8(0xf0, 0xee, 0xff);

/// 表示中のセリフの進み具合
#[derive(Resource, Default)]
pub struct Typewriter {
    shown: usize,
    elapsed_ms: f32,
    done: bool,
}

impl Typewriter {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Component)]
pub struct DialogueRoot;

#[derive(Component)]
pub struct SpeakerBadge;

#[derive(Component)]
pub struct SpeakerText;

#[derive(Component)]
pub struct DialogueText;

#[derive(Component)]
pub struct GhostSpan;

/// 完了カーソル ▼ (中身は点滅させる色)
#[derive(Component)]
pub struct CursorSpan(pub Color);

/// トランジション中でなく、表示すべきセリフがあるときだけウィンドウを出す
pub fn dialogue_visible(story: &Story, transition: &Transition) -> bool {
    !story.ended && !transition.active() && story.dialogue().is_some()
}

pub fn spawn_dialogue_box(mut commands: Commands, font: Res<UiFont>) {
    let text_font = TextFont::from(font.0.clone()).with_font_size(20.0);
    let speaker_font = TextFont::from(font.0.clone()).with_font_size(17.0);

    commands.spawn((
        DialogueRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::FlexEnd,
            justify_content: JustifyContent::Center,
            padding: UiRect::bottom(vh(4)),
            ..default()
        },
        GlobalZIndex(20),
        Visibility::Hidden,
        children![(
            // ─ RPGらしい二重枠ボーダー ─
            Node {
                width: percent(90),
                max_width: px(900),
                min_height: px(120),
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(4)),
                padding: UiRect::all(px(2)),
                ..default()
            },
            BorderColor::all(FRAME_LIGHT),
            BackgroundColor(BOX_BG),
            children![
                // 話者名バッジ
                (
                    SpeakerBadge,
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-38),
                        left: px(12),
                        border: UiRect {
                            left: px(3),
                            right: px(3),
                            top: px(3),
                            bottom: px(0),
                        },
                        border_radius: BorderRadius::top(px(3)),
                        padding: UiRect::all(px(2)),
                        ..default()
                    },
                    BorderColor::all(FRAME_LIGHT),
                    BackgroundColor(BADGE_BG),
                    children![(
                        Node {
                            border: UiRect {
                                left: px(2),
                                right: px(2),
                                top: px(2),
                                bottom: px(0),
                            },
                            padding: UiRect::new(px(10), px(14), px(2), px(2)),
                            ..default()
                        },
                        BorderColor::all(FRAME_INNER),
                        children![(
                            SpeakerText,
                            Text::new(""),
                            speaker_font,
                            TextColor(TEXT_COLOR),
                        )],
                    )],
                ),
                // 内側の紫ライン + テキストエリア
                (
                    Node {
                        flex_grow: 1.0,
                        border: UiRect::all(px(2)),
                        padding: UiRect::new(px(22), px(22), px(14), px(16)),
                        ..default()
                    },
                    BorderColor::all(FRAME_INNER),
                    children![(
                        DialogueText,
                        Text::new(""),
                        text_font.clone(),
                        bevy::text::LineHeight::RelativeToFont(1.9),
                        TextLayout::linebreak(crate::panel::WRAP),
                        TextColor(TEXT_COLOR),
                        children![
                            // 未表示テキストを透明で確保（レイアウトシフト防止）
                            (
                                GhostSpan,
                                TextSpan::new(""),
                                text_font.clone(),
                                TextColor(Color::NONE),
                            ),
                            // 完了カーソル
                            (
                                CursorSpan(FRAME_LIGHT),
                                TextSpan::new(""),
                                text_font.with_font_size(15.0),
                                TextColor(FRAME_LIGHT),
                            ),
                        ],
                    )],
                ),
            ],
        )],
    ));
}

/// 次のセリフ・シーンへ進む
fn advance(story: &mut Story, transition: &mut Transition, typewriter: &mut Typewriter) {
    typewriter.reset();

    let next_dialogue = story.dialogue_idx + 1;
    if next_dialogue < story.scene().dialogues.len() {
        // 同じシーン内の次のセリフへ
        story.dialogue_idx = next_dialogue;
        return;
    }

    // セリフが尽きたので次のシーンへ遷移するか確認
    if let Some(next) = story.scene().next.clone()
        && let Some(next_idx) = story.find_scene(&next.scene)
    {
        let kind = next.transition_out.unwrap_or_default();
        transition.go_to_scene(story, next_idx, kind);
        return;
    }

    // 次のシーンがない（または next の指定がない）なら終了
    story.ended = true;
}

pub fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    blocked: Res<PointerBlocked>,
    mut story: ResMut<Story>,
    mut transition: ResMut<Transition>,
    mut typewriter: ResMut<Typewriter>,
) {
    let pressed = keys.any_just_pressed([KeyCode::Enter, KeyCode::Space, KeyCode::KeyZ])
        // 本や背表紙の上でのクリックは本の操作に使う
        || (mouse.just_pressed(MouseButton::Left) && !blocked.0);
    if !pressed || !dialogue_visible(&story, &transition) {
        return;
    }

    if typewriter.done {
        advance(&mut story, &mut transition, &mut typewriter);
    } else {
        // 表示中: 全文を即座に表示
        typewriter.shown = usize::MAX;
        typewriter.done = true;
    }
}

pub fn tick_typewriter(
    time: Res<Time>,
    story: Res<Story>,
    transition: Res<Transition>,
    mut typewriter: ResMut<Typewriter>,
) {
    if typewriter.done || !dialogue_visible(&story, &transition) {
        return;
    }
    let Some(dialogue) = story.dialogue() else {
        return;
    };
    let speed = dialogue.speed.unwrap_or(DEFAULT_SPEED).max(1.0);
    let total = dialogue.text.chars().count();

    typewriter.elapsed_ms += time.delta_secs() * 1000.0;
    while typewriter.elapsed_ms >= speed && typewriter.shown < total {
        typewriter.elapsed_ms -= speed;
        typewriter.shown += 1;
    }
    if typewriter.shown >= total {
        typewriter.done = true;
    }
}

#[allow(clippy::type_complexity)]
pub fn draw_dialogue(
    time: Res<Time>,
    story: Res<Story>,
    transition: Res<Transition>,
    typewriter: Res<Typewriter>,
    mut root: Single<&mut Visibility, With<DialogueRoot>>,
    mut badge: Single<&mut Node, With<SpeakerBadge>>,
    mut speakers: Query<&mut Text, (With<SpeakerText>, Without<DialogueText>)>,
    mut texts: Query<
        (&mut Text, Option<&WrapEm>, &Children),
        (With<DialogueText>, Without<SpeakerText>),
    >,
    mut ghosts: Query<&mut TextSpan, (With<GhostSpan>, Without<CursorSpan>)>,
    mut cursors: Query<(&mut TextSpan, &mut TextColor, &CursorSpan), Without<GhostSpan>>,
) {
    let dialogue = story
        .dialogue()
        .filter(|_| dialogue_visible(&story, &transition));

    // 背景に会話欄があるシーンでは画面下のウィンドウは出さない
    let use_box = panel_for(&story.scene().background).is_none();
    **root = if dialogue.is_some() && use_box {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    let speaker = dialogue.and_then(|d| d.speaker.as_deref());
    badge.display = if speaker.is_some() {
        Display::Flex
    } else {
        Display::None
    };

    let full = dialogue.map_or("", |d| d.text.as_str());
    let split = full
        .char_indices()
        .nth(typewriter.shown)
        .map_or(full.len(), |(i, _)| i);

    for mut text in &mut speakers {
        set_if_changed(&mut text.0, speaker.unwrap_or_default());
    }
    for (mut text, wrap, children) in &mut texts {
        let (shown, rest) = match wrap {
            Some(em) => wrap_split(full, split, em.0),
            None => {
                let (shown, rest) = full.split_at(split);
                (shown.to_string(), rest.to_string())
            }
        };
        set_if_changed(&mut text.0, &shown);
        // 未表示テキストを透明で確保（レイアウトシフト防止）
        for &child in children {
            if let Ok(mut span) = ghosts.get_mut(child) {
                set_if_changed(&mut span.0, &rest);
            }
        }
    }

    // 完了カーソル ▼ を点滅させる
    let done = dialogue.is_some() && typewriter.done;
    let blink = 0.6 + 0.4 * (time.elapsed_secs() / 0.9 * std::f32::consts::TAU).cos();
    for (mut span, mut color, cursor) in &mut cursors {
        set_if_changed(&mut span.0, if done { " ▼" } else { "" });
        color.0 = cursor.0.with_alpha(blink);
    }
}

/// 同じ内容なら書き換えない (テキストの再レイアウトを避ける)
fn set_if_changed(dst: &mut String, src: &str) {
    if dst != src {
        *dst = src.to_string();
    }
}
