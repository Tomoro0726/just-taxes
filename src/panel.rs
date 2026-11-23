//! 背景のドット絵の一角をチャット画面として上書きし、セリフを表示する。
//!
//! 座標はすべて背景のドット座標系 (480×270) で指定し、
//! 画面サイズに合わせてステージと一緒に拡大する。

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::text::{LineBreak, LineHeight};
use bevy::window::PrimaryWindow;

use crate::counter::{BUTTON_BAR, Counter};
use crate::dialogue::{CursorSpan, DialogueText, GhostSpan, dialogue_visible};
use crate::scenario::{Dialogue, Story};
use crate::transition::Transition;
use crate::wrap::{WrapEm, wrap};
use crate::{DOT_H, DOT_W, UiFont};

/// ドット座標系での矩形 (x, y は左上)
#[derive(Clone, Copy)]
pub struct DotRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub(crate) const fn rect(x: f32, y: f32, w: f32, h: f32) -> DotRect {
    DotRect { x, y, w, h }
}

pub struct PanelLayout {
    /// このチャット画面を重ねる背景
    pub background: &'static str,
    /// チャット画面の範囲 (背景に描かれた会話欄ごと上書きする)
    area: DotRect,
    /// セリフの文字の高さ (ドット)
    font_dots: f32,
    /// セリフの行送り (ドット)
    line_dots: f32,
    /// 話者名・ナレーションの文字の高さ (ドット)
    small_dots: f32,
    palette: ChatPalette,
}

struct ChatPalette {
    background: Color,
    /// 表示中のセリフの枠 (背景に描かれていた会話欄と同じ色)
    frame: Color,
    /// 話し終えたセリフの枠 (少し暗くして表示中のセリフと区別する)
    past_frame: Color,
    frame_edge: Color,
    ink: Color,
    narration: Color,
    /// 主人公の名札の四角 (背景に描かれていた青いアイコンと同じ色)
    player_marker: Color,
    /// それ以外の話者に割り当てる名札の四角の色
    markers: &'static [Color],
}

pub const PANEL_LAYOUTS: &[PanelLayout] = &[PanelLayout {
    background: "main",
    // 左下の暗い領域全体 (上端の角丸と外枠は残す)
    area: rect(2.0, 136.0, 124.0, 132.0),
    font_dots: 8.0,
    line_dots: 10.0,
    small_dots: 6.0,
    palette: ChatPalette {
        background: Color::srgb_u8(0x4d, 0x2b, 0x32),
        frame: Color::srgb_u8(0xad, 0x77, 0x57),
        past_frame: Color::srgb_u8(0x94, 0x64, 0x4b),
        frame_edge: Color::srgb_u8(0x24, 0x15, 0x27),
        ink: Color::srgb_u8(0x17, 0x20, 0x38),
        narration: Color::srgb_u8(0xc8, 0xa8, 0x90),
        player_marker: Color::srgb_u8(0x25, 0x3a, 0x5e),
        markers: &[
            Color::srgb_u8(0x7a, 0x2e, 0x3a),
            Color::srgb_u8(0x2e, 0x5e, 0x3a),
            Color::srgb_u8(0x6b, 0x4e, 0x8a),
            Color::srgb_u8(0x8a, 0x5a, 0x2e),
        ],
    },
}];

/// 日本語は単語辞書なしでは区切れないので、単語で切れなければ文字単位で折り返す
pub const WRAP: LineBreak = LineBreak::WordOrCharacter;

/// チャット画面・セリフ枠の内側の余白と枠線 (ドット)
const CHAT_PAD_X: f32 = 3.0;
const FRAME_PAD_X: f32 = 3.0;
const FRAME_BORDER: f32 = 1.0;

pub fn panel_for(background: &str) -> Option<&'static PanelLayout> {
    PANEL_LAYOUTS.iter().find(|p| p.background == background)
}

#[derive(Component)]
pub struct PanelRoot(&'static str);

/// ステージ (背景が表示されている 16:9 の領域) にぴったり重ねる UI の根
#[derive(Component)]
pub struct StageNode;

/// ステージの拡大率に合わせて文字サイズを変える (値はドット単位)
#[derive(Component)]
pub struct DotFont(pub(crate) f32);

/// ステージの拡大率に合わせて大きさ・余白・枠線を変える (値はドット単位)
#[derive(Component, Default, Clone, Copy)]
pub struct DotStyle {
    pub(crate) size: Option<Vec2>,
    pub(crate) height: Option<f32>,
    pub(crate) min_width: Option<f32>,
    pub(crate) max_width: Option<f32>,
    pub(crate) padding: Vec2,
    pub(crate) margin_top: f32,
    pub(crate) gap: f32,
    pub(crate) border: f32,
    pub(crate) radius: f32,
    /// 下辺の枠線なし・上側だけ角丸 (セリフ枠の上に載せる名札用)
    pub(crate) open_bottom: bool,
}

/// チャット画面のスクロール領域
#[derive(Component)]
pub struct ChatLog {
    layout: &'static PanelLayout,
    /// 最新のセリフに追従して一番下を表示するか
    follow: bool,
}

/// チャットのメッセージを並べるリスト
#[derive(Component)]
pub struct ChatLogList;

/// ドット座標の矩形を、ステージに対する割合で配置する
pub(crate) fn dot_node(r: DotRect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: percent(r.x / DOT_W * 100.0),
        top: percent(r.y / DOT_H * 100.0),
        width: percent(r.w / DOT_W * 100.0),
        height: percent(r.h / DOT_H * 100.0),
        ..default()
    }
}

pub fn spawn_panels(mut commands: Commands) {
    for layout in PANEL_LAYOUTS {
        commands.spawn((
            PanelRoot(layout.background),
            StageNode,
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            // トランジションや Fin 画面より下
            GlobalZIndex(10),
            Visibility::Hidden,
            children![(
                ChatLog {
                    layout,
                    follow: true,
                },
                Node {
                    overflow: Overflow::scroll_y(),
                    // 既定の Stretch だとリストの高さが表示領域に固定され、
                    // あふれたセリフが上にはみ出すだけでスクロール量が 0 になる
                    align_items: AlignItems::FlexStart,
                    ..dot_node(layout.area)
                },
                DotStyle {
                    padding: Vec2::new(CHAT_PAD_X, 2.0),
                    ..default()
                },
                BackgroundColor(layout.palette.background),
                ScrollPosition::default(),
                children![(
                    ChatLogList,
                    // メッセージが少ないうちは下に寄せ、あふれたら上へスクロールできるようにする
                    Node {
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::FlexEnd,
                        width: percent(100),
                        min_height: percent(100),
                        ..default()
                    },
                )],
            )],
        ));
    }
}

/// ステージの拡大率 (1 ドットあたりの論理ピクセル) と左上の位置
pub(crate) fn stage_transform(window: &Window) -> (f32, Vec2) {
    let (w, h) = (window.width(), window.height());
    // カメラの ScalingMode::AutoMin と同じく、短い辺に合わせて拡大する
    let scale = (w / DOT_W).min(h / DOT_H);
    let origin = Vec2::new(w - DOT_W * scale, h - DOT_H * scale) / 2.0;
    (scale, origin)
}

/// チャット画面をステージ (背景が表示されている 16:9 の領域) にぴったり重ねる
#[allow(clippy::too_many_arguments)]
pub fn layout_panels(
    window: Single<&Window, With<PrimaryWindow>>,
    story: Res<Story>,
    mut stages: Query<&mut Node, With<StageNode>>,
    mut roots: Query<(&PanelRoot, &mut Visibility)>,
    mut fonts: Query<(&DotFont, &mut TextFont)>,
    mut styles: Query<(&DotStyle, &mut Node, Option<&ChatLog>), Without<StageNode>>,
    counter: Res<Counter>,
    transition: Res<Transition>,
) {
    let button_bar = if counter.judging && !transition.active() {
        BUTTON_BAR.h
    } else {
        0.0
    };
    let (scale, origin) = stage_transform(&window);

    for mut node in &mut stages {
        node.left = px(origin.x);
        node.top = px(origin.y);
        node.width = px(DOT_W * scale);
        node.height = px(DOT_H * scale);
    }

    for (root, mut visibility) in &mut roots {
        *visibility = if story.scene().background == root.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (dots, mut font) in &mut fonts {
        let size = FontSize::Px(dots.0 * scale);
        if font.font_size != size {
            font.font_size = size;
        }
    }

    for (style, mut node, log) in &mut styles {
        if let Some(log) = log {
            node.height = percent((log.layout.area.h - button_bar) / DOT_H * 100.0);
        }
        if let Some(size) = style.size {
            node.width = px(size.x * scale);
            node.height = px(size.y * scale);
        }
        if let Some(height) = style.height {
            node.height = px(height * scale);
        }
        if let Some(min_width) = style.min_width {
            node.min_width = px(min_width * scale);
        }
        if let Some(max_width) = style.max_width {
            node.max_width = px(max_width * scale);
        }
        node.padding = UiRect::axes(px(style.padding.x * scale), px(style.padding.y * scale));
        node.margin = UiRect::top(px(style.margin_top * scale));
        node.column_gap = px(style.gap * scale);
        let (border, radius) = (px(style.border * scale), px(style.radius * scale));
        if style.open_bottom {
            node.border = UiRect {
                bottom: px(0),
                ..UiRect::all(border)
            };
            node.border_radius = BorderRadius::top(radius);
        } else {
            node.border = UiRect::all(border);
            node.border_radius = BorderRadius::all(radius);
        }
    }
}

/// 話者ごとに決まった名札の四角の色
fn marker_color(layout: &PanelLayout, name: &str, player: Option<&str>) -> Color {
    let p = &layout.palette;
    if player == Some(name) {
        return p.player_marker;
    }
    let hash = name
        .chars()
        .fold(0usize, |h, c| h.wrapping_mul(31).wrapping_add(c as usize));
    p.markers[hash % p.markers.len()]
}

/// 1 件分のセリフを作る。`live` なら表示中のセリフとしてタイプライター表示の対象にする
fn spawn_message(
    list: &mut ChildSpawnerCommands,
    layout: &'static PanelLayout,
    font: &Handle<Font>,
    player: Option<&str>,
    dialogue: &Dialogue,
    first_of_speaker: bool,
    live: bool,
) {
    let p = &layout.palette;
    let text_font = TextFont::from(font.clone());
    // 1 行に入る文字数 (全角 = 1em)。自前で改行して幅に収める (末尾の ▼ の分を 1 文字空ける)
    let frame_em =
        (layout.area.w - 2.0 * (CHAT_PAD_X + FRAME_PAD_X + FRAME_BORDER)) / layout.font_dots - 1.0;
    let narration_dots = layout.small_dots + 1.0;
    let narration_em = (layout.area.w - 2.0 * CHAT_PAD_X) / narration_dots - 1.0;
    // 表示中のセリフは、未表示の部分を透明で確保してから 1 文字ずつ出す
    let text = |em: f32| {
        if live {
            String::new()
        } else {
            wrap(&dialogue.text, em)
        }
    };

    let Some(name) = dialogue.speaker.as_deref() else {
        // ナレーション: 枠なしで中央に小さく出す
        list.spawn((
            Node {
                justify_content: JustifyContent::Center,
                ..default()
            },
            DotStyle {
                margin_top: 4.0,
                ..default()
            },
        ))
        .with_children(|row| {
            let mut t = row.spawn((
                Text::new(text(narration_em)),
                text_font.clone(),
                DotFont(narration_dots),
                TextColor(p.narration),
                TextLayout::new(Justify::Center, LineBreak::NoWrap),
            ));
            if live {
                t.insert((DialogueText, WrapEm(narration_em)))
                    .with_children(|t| {
                        live_spans(t, &text_font, narration_dots, p.narration, layout)
                    });
            }
        });
        return;
    };

    let fill = if live { p.frame } else { p.past_frame };
    list.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            flex_shrink: 0.0,
            ..default()
        },
        DotStyle {
            margin_top: if first_of_speaker { 4.0 } else { 2.0 },
            ..default()
        },
    ))
    .with_children(|message| {
        // 名札 (背景の上の欄と同じ、色付きの四角 + 名前)。同じ話者が続くときは省く
        if first_of_speaker {
            message
                .spawn((
                    Node {
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    DotStyle {
                        padding: Vec2::new(2.0, 1.0),
                        gap: 2.0,
                        border: FRAME_BORDER,
                        radius: 2.0,
                        open_bottom: true,
                        ..default()
                    },
                    BorderColor::all(p.frame_edge),
                    BackgroundColor(fill),
                ))
                .with_children(|tag| {
                    tag.spawn((
                        Node::default(),
                        DotStyle {
                            size: Some(Vec2::splat(5.0)),
                            border: 1.0,
                            ..default()
                        },
                        BorderColor::all(p.frame_edge),
                        BackgroundColor(marker_color(layout, name, player)),
                    ));
                    tag.spawn((
                        Text::new(name),
                        text_font.clone(),
                        DotFont(layout.small_dots),
                        TextColor(p.ink),
                    ));
                });
        }

        // セリフ枠 (背景の下の欄と同じ枠)
        message
            .spawn((
                Node {
                    width: percent(100),
                    ..default()
                },
                DotStyle {
                    padding: Vec2::new(FRAME_PAD_X, 1.0),
                    border: FRAME_BORDER,
                    radius: 2.0,
                    ..default()
                },
                BorderColor::all(p.frame_edge),
                BackgroundColor(fill),
            ))
            .with_children(|frame| {
                let mut t = frame.spawn((
                    Text::new(text(frame_em)),
                    text_font.clone(),
                    DotFont(layout.font_dots),
                    LineHeight::RelativeToFont(layout.line_dots / layout.font_dots),
                    TextLayout::linebreak(LineBreak::NoWrap),
                    TextColor(p.ink),
                ));
                if live {
                    t.insert((DialogueText, WrapEm(frame_em)))
                        .with_children(|t| {
                            live_spans(t, &text_font, layout.font_dots, p.ink, layout)
                        });
                }
            });
    });
}

/// タイプライター表示用の「未表示部分」と「完了カーソル」
fn live_spans(
    t: &mut ChildSpawnerCommands,
    font: &TextFont,
    font_dots: f32,
    ink: Color,
    layout: &PanelLayout,
) {
    t.spawn((
        GhostSpan,
        TextSpan::new(""),
        font.clone(),
        DotFont(font_dots),
        TextColor(Color::NONE),
    ));
    t.spawn((
        CursorSpan(ink),
        TextSpan::new(""),
        font.clone(),
        DotFont(layout.small_dots),
        TextColor(ink),
    ));
}

/// 現在のシーンのセリフをチャットとして並べる (最後の 1 件は表示中のセリフ)
pub fn update_chat_log(
    mut commands: Commands,
    font: Res<UiFont>,
    story: Res<Story>,
    transition: Res<Transition>,
    mut shown: Local<Option<(usize, usize, bool)>>,
    mut logs: Query<(&mut ChatLog, &Children)>,
    lists: Query<Entity, With<ChatLogList>>,
) {
    let dialogues = &story.scene().dialogues;
    // 物語が終わったら最後のセリフも話し終えた扱いにする
    let past = (story.dialogue_idx + usize::from(story.ended)).min(dialogues.len());
    let live = dialogue_visible(&story, &transition);
    let key = (story.scene_idx, past, live);
    if *shown == Some(key) {
        return;
    }
    *shown = Some(key);

    let count = if live { past + 1 } else { past };
    let player = story.chapter.player.as_deref();

    for (mut log, children) in &mut logs {
        let layout = log.layout;
        log.follow = true;
        let Some(list) = children.iter().find(|&c| lists.contains(c)) else {
            continue;
        };
        commands.entity(list).despawn_children();
        commands.entity(list).with_children(|list| {
            for (i, dialogue) in dialogues[..count].iter().enumerate() {
                let first_of_speaker = i == 0 || dialogues[i - 1].speaker != dialogue.speaker;
                let is_live = live && i == past;
                spawn_message(
                    list,
                    layout,
                    &font.0,
                    player,
                    dialogue,
                    first_of_speaker,
                    is_live,
                );
            }
        });
    }
}

/// マウスホイールでチャットをさかのぼる
pub fn scroll_chat_log(
    mut wheel: MessageReader<MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut logs: Query<(
        &mut ChatLog,
        &mut ScrollPosition,
        &ComputedNode,
        &InheritedVisibility,
        &Children,
    )>,
    lists: Query<&ComputedNode, With<ChatLogList>>,
) {
    let (scale, origin) = stage_transform(&window);
    let cursor = window.cursor_position();

    let delta: f32 = wheel
        .read()
        .map(|e| match e.unit {
            // 1 ノッチで 3 行分
            MouseScrollUnit::Line => e.y * 3.0 * 10.0 * scale,
            MouseScrollUnit::Pixel => e.y,
        })
        .sum();

    for (mut log, mut scroll, computed, visible, children) in &mut logs {
        // スクロールできる量 = 並べたセリフの高さ - 表示領域の高さ。
        // ComputedNode::content_size は子のはみ出しも含んで大きめに出るので使わない
        let Some(list) = children.iter().find_map(|c| lists.get(c).ok()) else {
            continue;
        };
        let inset = computed.content_inset();
        let view_height = computed.size().y - inset.min_inset.y - inset.max_inset.y;
        let max = ((list.size().y - view_height) * computed.inverse_scale_factor()).max(0.0);

        let r = log.layout.area;
        let min = origin + Vec2::new(r.x, r.y) * scale;
        let over_log = cursor.is_some_and(|c| {
            Rect::from_corners(min, min + Vec2::new(r.w, r.h) * scale).contains(c)
        });

        if delta != 0.0 && over_log && visible.get() {
            let current = if log.follow { max } else { scroll.y.min(max) };
            // ホイールを上に回すと過去のセリフへ戻る
            scroll.y = (current - delta).clamp(0.0, max);
            log.follow = scroll.y >= max - 0.5;
        }
        scroll.y = if log.follow { max } else { scroll.y.min(max) };
    }
}
