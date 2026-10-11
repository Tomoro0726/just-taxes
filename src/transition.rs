//! シーン切り替え時のトランジション (フェード / ワイプ)。

use bevy::prelude::*;

use crate::scenario::{Story, TransitionType};

/// アニメーション時間 (秒)
const DURATION: f32 = 0.8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// 画面を黒で覆っていく
    Out,
    /// 黒を取り除いて次の画面を見せる
    In,
}

struct Phase {
    kind: TransitionType,
    direction: Direction,
    elapsed: f32,
    /// Out 完了後に切り替えるシーン
    next_scene: Option<usize>,
}

#[derive(Resource, Default)]
pub struct Transition {
    phase: Option<Phase>,
}

impl Transition {
    pub fn active(&self) -> bool {
        self.phase.is_some()
    }

    pub fn play_in(&mut self, kind: TransitionType) {
        self.phase = (kind != TransitionType::None).then_some(Phase {
            kind,
            direction: Direction::In,
            elapsed: 0.0,
            next_scene: None,
        });
    }

    /// `kind` で画面を覆ってから `next_scene` へ切り替え、そのシーンの transition_in で開ける
    pub fn go_to_scene(&mut self, story: &mut Story, next_scene: usize, kind: TransitionType) {
        if kind == TransitionType::None {
            switch_scene(story, self, next_scene);
            return;
        }
        self.phase = Some(Phase {
            kind,
            direction: Direction::Out,
            elapsed: 0.0,
            next_scene: Some(next_scene),
        });
    }
}

fn switch_scene(story: &mut Story, transition: &mut Transition, next_scene: usize) {
    story.scene_idx = next_scene;
    story.dialogue_idx = 0;
    let kind = story.scene().transition_in.unwrap_or_default();
    transition.play_in(kind);
}

#[derive(Component)]
pub struct TransitionOverlay;

pub fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        TransitionOverlay,
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        BackgroundColor(Color::BLACK),
        // 背景より上、ダイアログより下
        GlobalZIndex(50),
        Visibility::Hidden,
    ));
}

pub fn tick_transition(
    time: Res<Time>,
    mut transition: ResMut<Transition>,
    mut story: ResMut<Story>,
) {
    let Some(phase) = transition.phase.as_mut() else {
        return;
    };
    phase.elapsed += time.delta_secs();
    if phase.elapsed < DURATION {
        return;
    }
    match (phase.direction, phase.next_scene) {
        (Direction::Out, Some(next)) => switch_scene(&mut story, &mut transition, next),
        _ => transition.phase = None,
    }
}

// CSS の `ease` に近いカーブ
fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

// CSS の cubic-bezier(0.77, 0, 0.175, 1) に近いカーブ
fn ease_in_out_quart(t: f32) -> f32 {
    if t < 0.5 {
        8.0 * t.powi(4)
    } else {
        1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
    }
}

#[derive(Clone, Copy)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

fn edge_mut(rect: &mut UiRect, edge: Edge) -> &mut Val {
    match edge {
        Edge::Left => &mut rect.left,
        Edge::Right => &mut rect.right,
        Edge::Top => &mut rect.top,
        Edge::Bottom => &mut rect.bottom,
    }
}

pub fn draw_overlay(
    transition: Res<Transition>,
    mut overlay: Single<
        (&mut Node, &mut BackgroundColor, &mut Visibility),
        With<TransitionOverlay>,
    >,
) {
    let (node, color, visibility) = &mut *overlay;
    let Some(phase) = &transition.phase else {
        **visibility = Visibility::Hidden;
        return;
    };
    **visibility = Visibility::Visible;

    let t = (phase.elapsed / DURATION).clamp(0.0, 1.0);
    let mut rect = UiRect::all(Val::Percent(0.0));
    let mut alpha = 1.0;

    // ワイプ: Out は覆う側の辺が 100% → 0%、In は反対側の辺が 0% → 100% に動く
    let edges = match phase.kind {
        TransitionType::WipeLeft => Some((Edge::Left, Edge::Right)),
        TransitionType::WipeRight => Some((Edge::Right, Edge::Left)),
        TransitionType::WipeUp => Some((Edge::Top, Edge::Bottom)),
        TransitionType::WipeDown => Some((Edge::Bottom, Edge::Top)),
        TransitionType::Fade | TransitionType::None => None,
    };

    match (edges, phase.direction) {
        (Some((out_edge, _)), Direction::Out) => {
            *edge_mut(&mut rect, out_edge) = Val::Percent((1.0 - ease_in_out_quart(t)) * 100.0);
        }
        (Some((_, in_edge)), Direction::In) => {
            *edge_mut(&mut rect, in_edge) = Val::Percent(ease_in_out_quart(t) * 100.0);
        }
        (None, Direction::Out) => alpha = ease(t),
        (None, Direction::In) => alpha = 1.0 - ease(t),
    }

    node.left = rect.left;
    node.right = rect.right;
    node.top = rect.top;
    node.bottom = rect.bottom;
    color.0 = Color::BLACK.with_alpha(alpha);
}
