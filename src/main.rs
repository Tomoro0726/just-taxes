mod books;
mod dialogue;
mod dot_css;
mod panel;
mod scenario;
mod transition;
mod wrap;

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use scenario::{LoadError, Story};
use transition::Transition;

/// 背景ドット絵の解像度 (480×270 dot)
pub const DOT_W: f32 = 480.0;
pub const DOT_H: f32 = 270.0;

const END_COLOR: Color = Color::srgb_u8(0xc8, 0xc0, 0xe8);

#[derive(Resource)]
pub struct UiFont(pub Handle<Font>);

fn main() {
    // 起動引数 "chapterId[/sceneId[/dialogueIdx]]" で特定シーン・セリフへ直接ジャンプできる
    // 例: cargo run -- chapter1/town/2
    let arg = std::env::args().nth(1);

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Just Taxes".into(),
                    resolution: (1280, 720).into(),
                    ..default()
                }),
                ..default()
            })
            // ピクセルアート: スムージングなしで拡大描画
            .set(ImagePlugin::default_nearest()),
    )
    .add_plugins(dot_css::DotCssPlugin)
    .insert_resource(ClearColor(Color::BLACK))
    .init_resource::<Transition>()
    .init_resource::<dialogue::Typewriter>()
    .init_resource::<books::Books>()
    .init_resource::<books::PointerBlocked>()
    .add_systems(Startup, (setup, transition::spawn_overlay));

    match scenario::load_story(arg.as_deref()) {
        Ok(story) => {
            // 通常の最初のシーンだけ、演出としてトランジションインする
            // （引数でのデバッグジャンプは即座に表示したいので省略）
            if story.scene_idx == 0
                && let Some(kind) = story.scene().transition_in
            {
                app.world_mut().resource_mut::<Transition>().play_in(kind);
            }
            app.insert_resource(story)
                .add_systems(
                    Startup,
                    (
                        spawn_end_screen,
                        dialogue::spawn_dialogue_box,
                        panel::spawn_panels,
                        books::spawn_books,
                    )
                        .after(setup),
                )
                .add_systems(
                    Update,
                    (
                        books::book_input,
                        dialogue::handle_input,
                        transition::tick_transition,
                        dialogue::tick_typewriter,
                        (
                            update_background,
                            dialogue::draw_dialogue,
                            panel::update_chat_log,
                            panel::scroll_chat_log,
                            books::scroll_books,
                            books::draw_books,
                            panel::layout_panels,
                            transition::draw_overlay,
                            draw_end_screen,
                        ),
                    )
                        .chain(),
                );
        }
        Err(message) => {
            error!("{message}");
            app.insert_resource(LoadError(message))
                .add_systems(Startup, spawn_error_screen.after(setup));
        }
    }

    app.run();
}

#[derive(Component)]
struct Background {
    name: String,
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // 480×270 dot のステージが 16:9 を保ったまま画面いっぱいに収まるカメラ
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: DOT_W,
                min_height: DOT_H,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));

    commands.spawn((
        Background {
            name: String::new(),
        },
        Sprite {
            custom_size: Some(Vec2::new(DOT_W, DOT_H)),
            ..default()
        },
        Visibility::Hidden,
    ));

    commands.insert_resource(UiFont(asset_server.load("fonts/DotGothic16-Regular.ttf")));
}

fn update_background(
    story: Res<Story>,
    asset_server: Res<AssetServer>,
    mut background: Single<(&mut Background, &mut Sprite, &mut Visibility)>,
) {
    let (bg, sprite, visibility) = &mut *background;
    let name = &story.scene().background;
    if bg.name == *name {
        return;
    }
    bg.name.clone_from(name);
    // 元データの CSS (box-shadow 形式) をそのまま読み込んで描画する
    sprite.image = asset_server.load(format!("scenes/{name}.css"));
    **visibility = Visibility::Visible;
}

// ─── 物語終了 ────────────────────────────────────────────────

#[derive(Component)]
struct EndScreen;

#[derive(Component)]
struct EndText {
    elapsed: f32,
}

fn full_screen_overlay() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}

fn spawn_end_screen(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        EndScreen,
        full_screen_overlay(),
        BackgroundColor(Color::BLACK.with_alpha(0.75)),
        GlobalZIndex(40),
        Visibility::Hidden,
        children![(
            EndText { elapsed: 0.0 },
            Text::new("―  F i n  ―"),
            TextFont::from(font.0.clone()).with_font_size(32.0),
            TextColor(END_COLOR.with_alpha(0.0)),
        )],
    ));
}

fn draw_end_screen(
    time: Res<Time>,
    story: Res<Story>,
    mut screen: Single<&mut Visibility, With<EndScreen>>,
    mut text: Single<(&mut EndText, &mut TextColor, &mut Node)>,
) {
    if !story.ended {
        **screen = Visibility::Hidden;
        return;
    }
    **screen = Visibility::Inherited;

    // 2秒かけて下からふわっと表示
    let (end, color, node) = &mut *text;
    end.elapsed += time.delta_secs();
    let t = (end.elapsed / 2.0).min(1.0);
    color.0 = END_COLOR.with_alpha(t);
    node.top = px(10.0 * (1.0 - t));
}

// ─── 読み込みエラー ──────────────────────────────────────────

fn spawn_error_screen(mut commands: Commands, font: Res<UiFont>, error: Res<LoadError>) {
    commands.spawn((
        full_screen_overlay(),
        BackgroundColor(Color::srgb_u8(5, 5, 12)),
        children![(
            Text::new(format!("⚠ {}", error.0)),
            TextFont::from(font.0.clone()).with_font_size(18.0),
            TextColor(Color::srgb_u8(0x88, 0x88, 0xaa)),
        )],
    ));
}
