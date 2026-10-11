//! 背景の本棚から資料の本を取り出し、机の上で広げて読む。
//!
//! - 本棚の背表紙をクリック → 机の上に広げる / 広げた本の × → 本棚に戻す
//! - 背表紙を机へドラッグ → ドロップした位置に広げる
//! - 広げた本の上の軸をドラッグ → 机の上で動かす / 本棚へドラッグで戻す
//! - Esc → 広げている本をすべて本棚に戻す
//!
//! 座標はすべて背景のドット座標系 (480×270)。

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::text::{LineBreak, LineHeight};
use bevy::window::PrimaryWindow;

use crate::panel::{DotFont, DotRect, DotStyle, StageNode, dot_node, rect, stage_transform};
use crate::scenario::Story;
use crate::transition::Transition;
use crate::wrap::wrap;
use crate::{DOT_H, DOT_W, UiFont};

// ─── 本の中身 ────────────────────────────────────────────────

pub enum Block {
    Heading(&'static str),
    /// 本文 (自前で折り返す)
    Text(&'static str),
    /// 3 列の帳簿 (見出し行・明細・合計)
    Ledger {
        head: (&'static str, &'static str, &'static str),
        rows: &'static [(&'static str, &'static str, &'static str)],
        total: (&'static str, &'static str),
    },
    /// 許可証のカード
    Permit {
        title: &'static str,
        lines: &'static [(&'static str, &'static str)],
    },
}

pub struct BookProp {
    /// どの背景に属するか
    pub background: &'static str,
    /// 本棚上の置き場所 (背表紙)
    pub shelf: DotRect,
    /// 机の上で本を広げられる範囲
    pub table: DotRect,
    pub blocks: &'static [Block],
}

/// 徴税所の机に敷かれた緑のマット
const DESK_MAT: DotRect = rect(138.0, 180.0, 172.0, 86.0);

pub const BOOK_PROPS: &[BookProp] = &[
    // 上段の元帳 4231〜4240 のうち 4235 番
    BookProp {
        background: "main",
        shelf: rect(291.0, 16.0, 7.0, 39.0),
        table: DESK_MAT,
        blocks: &[
            Block::Heading("元帳 No.4235　エルドブ商会"),
            Block::Ledger {
                head: ("", "前期", "今期"),
                rows: &[
                    ("硫黄 採掘高", "246,900", "123,456"),
                    ("人件費", "213,977", "213,977"),
                    ("水利", "12,400", "12,400"),
                    ("土地", "88,204", "88,204"),
                    ("保管", "41,660", "40,210"),
                    ("運送", "37,991", "75,982"),
                    ("護衛料", "12,050", "24,100"),
                    ("罰金", "0", "2,004"),
                ],
                total: ("納税額", "30,441"),
            },
            Block::Permit {
                title: "◎ 硫黄採掘許可証",
                lines: &[
                    ("名義", "エルドブ商会"),
                    ("番号", "1A235"),
                    ("期限", "4243年"),
                ],
            },
        ],
    },
    // 下段の LAW
    BookProp {
        background: "main",
        shelf: rect(289.0, 64.0, 13.0, 39.0),
        table: DESK_MAT,
        blocks: &[
            Block::Heading("税法典　抜粋"),
            Block::Text(
                "第七条　申告と帳簿の記載に相違あるときは、徴税官はその差額に相当する税を追徴する。",
            ),
            Block::Text(
                "第十二条　凶作・災害その他やむを得ない事由により、納税者の暮らしが立ち行かぬと認めるときは、徴税官は追徴の全部又は一部を翌期に繰り延べることができる。",
            ),
            Block::Text(
                "第十三条　前条の繰り延べを行ったときは、徴税官はその理由を元帳に記し、署名しなければならない。",
            ),
        ],
    },
];

// ─── 見た目 ──────────────────────────────────────────────────

/// 広げた本の大きさ (ドット)
const BOOK_SIZE: Vec2 = Vec2::new(160.0, 140.0);
/// 本がチャット欄 (左下) に重ならないようにする左端
const BOOK_MIN_LEFT: f32 = 128.0;
const BOOK_PAD: Vec2 = Vec2::new(8.0, 4.0);
/// 巻物の軸の太さ (ドット)。スクロール 0 で上が太く下が細い、最後まで読むとその逆
const ROD_MIN: f32 = 3.0;
const ROD_MAX: f32 = 11.0;
const CLOSE_SIZE: f32 = 10.0;
const CLOSE_INSET: f32 = 3.0;
const TEXT_DOTS: f32 = 7.0;
const HEADING_DOTS: f32 = 8.0;
const SMALL_DOTS: f32 = 6.0;
/// クリックとみなす移動量の上限 (論理ピクセル)
const CLICK_THRESHOLD: f32 = 6.0;

const PAPER: Color = Color::srgb_u8(0xd9, 0xc3, 0x97);
const PAPER_LIGHT: Color = Color::srgb_u8(0xef, 0xe0, 0xb8);
const LEATHER: Color = Color::srgb_u8(0x5c, 0x3a, 0x26);
const ROD: Color = Color::srgb_u8(0xb9, 0x8a, 0x5c);
const SEAL: Color = Color::srgb_u8(0xa5, 0x82, 0x4f);
const INK: Color = Color::srgb_u8(0x24, 0x1a, 0x12);
const INK_MUTED: Color = Color::srgb_u8(0x6b, 0x5a, 0x3f);
/// 本を取り出した後の、空いた本棚の奥の色
const SHELF_EMPTY: Color = Color::srgb_u8(0x34, 0x1c, 0x27);
const HIGHLIGHT: Color = Color::srgba(1.0, 244.0 / 255.0, 210.0 / 255.0, 0.75);
const DROPZONE_FILL: Color = Color::srgba(1.0, 230.0 / 255.0, 160.0 / 255.0, 0.12);

// ─── 状態 ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Shelf,
    /// 机の上。値は本の右下角 (本は机の空間側＝左上方向へ広がる)
    Table(Vec2),
}

struct Drag {
    book: usize,
    from_table: bool,
    /// 押した位置 (論理ピクセル)。クリックかドラッグかの判定に使う
    start: Vec2,
}

#[derive(Resource, Default)]
pub struct Books {
    places: Vec<Place>,
    /// 机の上の重なり順 (後ろほど手前)
    order: Vec<usize>,
    drag: Option<Drag>,
    /// カーソルが乗っている背表紙
    hover: Option<usize>,
    cursor: Option<Vec2>,
    background: String,
}

impl Books {
    fn bring_to_front(&mut self, book: usize) {
        self.order.retain(|&b| b != book);
        self.order.push(book);
    }

    /// カーソルの下にある、机の上の一番手前の本
    fn book_at(&self, p: Vec2) -> Option<usize> {
        self.order
            .iter()
            .rev()
            .copied()
            .find(|&b| match self.places[b] {
                Place::Table(anchor) => book_rect(anchor).contains(p),
                Place::Shelf => false,
            })
    }
}

/// 本や背表紙の上でのクリックは、セリフ送りに使わない
#[derive(Resource, Default)]
pub struct PointerBlocked(pub bool);

fn to_rect(r: DotRect) -> Rect {
    Rect::new(r.x, r.y, r.x + r.w, r.y + r.h)
}

fn book_rect(anchor: Vec2) -> Rect {
    Rect::from_corners(anchor - BOOK_SIZE, anchor)
}

fn close_rect(anchor: Vec2) -> Rect {
    let min = Vec2::new(
        anchor.x - CLOSE_INSET - CLOSE_SIZE,
        anchor.y - BOOK_SIZE.y + CLOSE_INSET,
    );
    Rect::from_corners(min, min + Vec2::splat(CLOSE_SIZE))
}

/// 上の軸 (ドラッグの持ち手)
fn handle_rect(anchor: Vec2) -> Rect {
    let book = book_rect(anchor);
    Rect::new(
        book.min.x,
        book.min.y,
        book.max.x,
        book.min.y + BOOK_PAD.y + ROD_MAX,
    )
}

/// ドロップ位置を机の範囲に収め、本が画面やチャット欄にはみ出さないようにする
fn clamp_anchor(p: Vec2, table: DotRect) -> Vec2 {
    let t = to_rect(table);
    let p = p.clamp(t.min, t.max);
    Vec2::new(
        p.x.clamp(BOOK_MIN_LEFT + BOOK_SIZE.x, DOT_W - 2.0),
        p.y.clamp(2.0 + BOOK_SIZE.y, DOT_H - 2.0),
    )
}

/// 本を置く操作の既定値: 机の右下角
fn default_anchor(table: DotRect) -> Vec2 {
    clamp_anchor(to_rect(table).max, table)
}

// ─── 入力 ────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn book_input(
    window: Single<&Window, With<PrimaryWindow>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    story: Res<Story>,
    transition: Res<Transition>,
    mut books: ResMut<Books>,
    mut blocked: ResMut<PointerBlocked>,
    mut scrolls: Query<(&BookScroll, &mut ScrollPosition)>,
) {
    if books.places.len() != BOOK_PROPS.len() {
        books.places = vec![Place::Shelf; BOOK_PROPS.len()];
    }

    // シーンが切り替わったら、取り出していた本は棚に戻す
    let background = &story.scene().background;
    if books.background != *background {
        books.background.clone_from(background);
        books.places.fill(Place::Shelf);
        books.order.clear();
        books.drag = None;
    }

    let (scale, origin) = stage_transform(&window);
    let cursor_px = window.cursor_position();
    let cursor = cursor_px.map(|c| (c - origin) / scale);
    books.cursor = cursor;
    books.hover = None;
    blocked.0 = false;

    if transition.active() {
        books.drag = None;
        return;
    }

    let in_scene: Vec<usize> = (0..BOOK_PROPS.len())
        .filter(|&b| BOOK_PROPS[b].background == *background)
        .collect();

    // Escape で読んでいる本を棚に戻す
    if keys.just_pressed(KeyCode::Escape) {
        for &b in &in_scene {
            books.places[b] = Place::Shelf;
        }
    }

    let Some(cursor) = cursor else {
        return;
    };

    let over_book = books.book_at(cursor);
    let over_shelf = in_scene.iter().copied().find(|&b| {
        books.places[b] == Place::Shelf && to_rect(BOOK_PROPS[b].shelf).contains(cursor)
    });
    if books.drag.is_none() {
        books.hover = over_shelf;
    }
    blocked.0 = books.drag.is_some() || over_book.is_some() || over_shelf.is_some();

    let reset_scroll = |scrolls: &mut Query<(&BookScroll, &mut ScrollPosition)>, book: usize| {
        for (scroll, mut position) in scrolls.iter_mut() {
            if scroll.0 == book {
                position.0 = Vec2::ZERO;
            }
        }
    };

    if mouse.just_pressed(MouseButton::Left) {
        if let Some(b) = over_book {
            let Place::Table(anchor) = books.places[b] else {
                unreachable!()
            };
            books.bring_to_front(b);
            if close_rect(anchor).contains(cursor) {
                books.places[b] = Place::Shelf;
            } else if handle_rect(anchor).contains(cursor) {
                books.drag = Some(Drag {
                    book: b,
                    from_table: true,
                    start: cursor_px.unwrap_or_default(),
                });
            }
        } else if let Some(b) = over_shelf {
            books.drag = Some(Drag {
                book: b,
                from_table: false,
                start: cursor_px.unwrap_or_default(),
            });
        }
    }

    if mouse.just_released(MouseButton::Left)
        && let Some(drag) = books.drag.take()
    {
        let prop = &BOOK_PROPS[drag.book];
        let moved = cursor_px.unwrap_or_default().distance(drag.start);
        if moved < CLICK_THRESHOLD {
            // ほぼ動いていなければクリックとみなし、反対側へトグルする
            if drag.from_table {
                books.places[drag.book] = Place::Shelf;
            } else {
                books.places[drag.book] = Place::Table(default_anchor(prop.table));
                books.bring_to_front(drag.book);
                reset_scroll(&mut scrolls, drag.book);
            }
        } else if to_rect(prop.shelf).contains(cursor) {
            // 本棚の上でドロップ ⇒ 出所に関わらず本棚へしまう
            books.places[drag.book] = Place::Shelf;
        } else if to_rect(prop.table).contains(cursor) {
            // 机の上でドロップ ⇒ 出所に関わらずその位置に置く/動かす
            books.places[drag.book] = Place::Table(clamp_anchor(cursor, prop.table));
            books.bring_to_front(drag.book);
            // 棚から出したときだけ最初のページに戻す（机の上での位置調整では読んでいた場所を保つ）
            if !drag.from_table {
                reset_scroll(&mut scrolls, drag.book);
            }
        }
    }
}

/// マウスホイールで広げた本を読み進める
pub fn scroll_books(
    mut wheel: MessageReader<MouseWheel>,
    window: Single<&Window, With<PrimaryWindow>>,
    books: Res<Books>,
    mut scrolls: Query<(&BookScroll, &mut ScrollPosition, &ComputedNode, &Children)>,
    lists: Query<&ComputedNode, With<BookList>>,
) {
    let (scale, _) = stage_transform(&window);
    let delta: f32 = wheel
        .read()
        .map(|e| match e.unit {
            MouseScrollUnit::Line => e.y * 3.0 * 10.0 * scale,
            MouseScrollUnit::Pixel => e.y,
        })
        .sum();
    let target = books.cursor.and_then(|c| books.book_at(c));

    for (scroll, mut position, computed, children) in &mut scrolls {
        let max = scroll_max(computed, children, &lists);
        if delta != 0.0 && target == Some(scroll.0) {
            position.y -= delta;
        }
        position.y = position.y.clamp(0.0, max);
    }
}

/// 中身の高さ - 表示領域の高さ (論理ピクセル)
fn scroll_max(
    computed: &ComputedNode,
    children: &Children,
    lists: &Query<&ComputedNode, With<BookList>>,
) -> f32 {
    let Some(list) = children.iter().find_map(|c| lists.get(c).ok()) else {
        return 0.0;
    };
    let inset = computed.content_inset();
    let view = computed.size().y - inset.min_inset.y - inset.max_inset.y;
    ((list.size().y - view) * computed.inverse_scale_factor()).max(0.0)
}

// ─── 描画 ────────────────────────────────────────────────────

#[derive(Component)]
pub struct BookRoot(&'static str);

#[derive(Component)]
pub struct ShelfSpot(usize);

#[derive(Component)]
pub struct Dropzone(usize);

#[derive(Component)]
pub struct BookPanel(usize);

#[derive(Component)]
pub struct BookRod {
    book: usize,
    top: bool,
}

#[derive(Component)]
pub struct BookScroll(usize);

#[derive(Component)]
pub struct BookList;

#[derive(Component)]
pub struct DragGhost;

#[allow(clippy::type_complexity)]
pub fn draw_books(
    story: Res<Story>,
    books: Res<Books>,
    mut roots: Query<(&BookRoot, &mut Visibility), (Without<BookPanel>, Without<DragGhost>)>,
    mut spots: Query<(&ShelfSpot, &mut BackgroundColor, &mut BorderColor), Without<Dropzone>>,
    mut zones: Query<(&Dropzone, &mut BackgroundColor, &mut BorderColor), Without<ShelfSpot>>,
    mut panels: Query<
        (&BookPanel, &mut Node, &mut Visibility, &mut GlobalZIndex),
        Without<DragGhost>,
    >,
    mut ghost: Single<(&mut Node, &mut Visibility), (With<DragGhost>, Without<BookPanel>)>,
    mut rods: Query<(&BookRod, &mut DotStyle)>,
    scrolls: Query<(&BookScroll, &ScrollPosition, &ComputedNode, &Children)>,
    lists: Query<&ComputedNode, With<BookList>>,
) {
    if books.places.len() != BOOK_PROPS.len() {
        return;
    }
    let background = &story.scene().background;
    for (root, mut visibility) in &mut roots {
        *visibility = if root.0 == background {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let dragging = books.drag.as_ref();

    for (spot, mut fill, mut edge) in &mut spots {
        let on_table = matches!(books.places[spot.0], Place::Table(_));
        // 本が机に出ていて棚が空のときは、背表紙を隠す
        fill.0 = if on_table { SHELF_EMPTY } else { Color::NONE };
        let highlight = !on_table && books.hover == Some(spot.0);
        *edge = BorderColor::all(if highlight { HIGHLIGHT } else { Color::NONE });
    }

    // 本を置ける机の範囲。普段は見えず、本棚からドラッグ中だけ光る
    for (zone, mut fill, mut edge) in &mut zones {
        let active = dragging.is_some_and(|d| d.book == zone.0 && !d.from_table);
        fill.0 = if active { DROPZONE_FILL } else { Color::NONE };
        *edge = BorderColor::all(if active { HIGHLIGHT } else { Color::NONE });
    }

    for (panel, mut node, mut visibility, mut z) in &mut panels {
        let Place::Table(anchor) = books.places[panel.0] else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        let min = anchor - BOOK_SIZE;
        node.left = percent(min.x / DOT_W * 100.0);
        node.top = percent(min.y / DOT_H * 100.0);
        node.width = percent(BOOK_SIZE.x / DOT_W * 100.0);
        node.height = percent(BOOK_SIZE.y / DOT_H * 100.0);
        let depth = books.order.iter().position(|&b| b == panel.0).unwrap_or(0);
        z.0 = 20 + depth as i32;
    }

    // ドラッグ中のプレビュー
    let (ghost_node, ghost_visibility) = &mut *ghost;
    match (dragging, books.cursor) {
        (Some(_), Some(c)) => {
            **ghost_visibility = Visibility::Inherited;
            ghost_node.left = percent((c.x - 6.5) / DOT_W * 100.0);
            ghost_node.top = percent((c.y - 8.5) / DOT_H * 100.0);
        }
        _ => **ghost_visibility = Visibility::Hidden,
    }

    // 巻物の軸の太さで、どこまで読んだかを表す
    for (rod, mut style) in &mut rods {
        let ratio = scrolls
            .iter()
            .find(|(s, ..)| s.0 == rod.book)
            .map(|(_, position, computed, children)| {
                let max = scroll_max(computed, children, &lists);
                if max > 0.0 {
                    (position.y / max).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        let height = if rod.top {
            ROD_MAX - (ROD_MAX - ROD_MIN) * ratio
        } else {
            ROD_MIN + (ROD_MAX - ROD_MIN) * ratio
        };
        style.height = Some(height);
    }
}

// ─── 生成 ────────────────────────────────────────────────────

fn text(font: &Handle<Font>, s: impl Into<String>, dots: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont::from(font.clone()),
        DotFont(dots),
        LineHeight::RelativeToFont(10.0 / 7.0),
        TextLayout::linebreak(LineBreak::NoWrap),
        TextColor(color),
    )
}

/// 右寄せの数値欄
fn value_cell(list: &mut ChildSpawnerCommands, font: &Handle<Font>, value: &str, color: Color) {
    list.spawn((
        Node {
            justify_content: JustifyContent::FlexEnd,
            flex_shrink: 0.0,
            ..default()
        },
        DotStyle {
            min_width: Some(34.0),
            ..default()
        },
    ))
    .with_child(text(font, value, TEXT_DOTS, color));
}

fn ledger_row(
    list: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    cells: (&str, &str, &str),
    color: Color,
) {
    list.spawn((
        Node::default(),
        DotStyle {
            margin_top: 2.0,
            ..default()
        },
    ))
    .with_children(|row| {
        row.spawn(Node {
            flex_grow: 1.0,
            ..default()
        })
        .with_child(text(font, cells.0, TEXT_DOTS, color));
        value_cell(row, font, cells.1, color);
        value_cell(row, font, cells.2, color);
    });
}

fn rule(list: &mut ChildSpawnerCommands, margin_top: f32) {
    list.spawn((
        Node {
            width: percent(100),
            ..default()
        },
        DotStyle {
            height: Some(1.0),
            margin_top,
            ..default()
        },
        BackgroundColor(LEATHER),
    ));
}

fn spawn_block(list: &mut ChildSpawnerCommands, font: &Handle<Font>, block: &Block) {
    // 本文の 1 行に入る文字数 (全角 = 1em)
    let text_em = (BOOK_SIZE.x - 2.0 * (BOOK_PAD.x + 1.0)) / TEXT_DOTS - 0.5;

    match block {
        Block::Heading(s) => {
            list.spawn((Node::default(), DotStyle::default()))
                .with_child(text(font, *s, HEADING_DOTS, INK));
        }
        Block::Text(s) => {
            list.spawn((
                Node::default(),
                DotStyle {
                    margin_top: 5.0,
                    ..default()
                },
            ))
            .with_child(text(font, wrap(s, text_em), TEXT_DOTS, INK));
        }
        Block::Ledger { head, rows, total } => {
            list.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                DotStyle {
                    margin_top: 3.0,
                    ..default()
                },
            ))
            .with_children(|ledger| {
                ledger_row(ledger, font, *head, INK_MUTED);
                for &row in *rows {
                    ledger_row(ledger, font, row, INK);
                }
                rule(ledger, 3.0);
                ledger_row(ledger, font, (total.0, "", total.1), INK);
            });
        }
        Block::Permit { title, lines } => {
            list.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_self: AlignSelf::FlexEnd,
                    width: percent(70),
                    ..default()
                },
                DotStyle {
                    padding: Vec2::new(6.0, 4.0),
                    margin_top: 8.0,
                    border: 1.0,
                    radius: 4.0,
                    ..default()
                },
                BackgroundColor(PAPER_LIGHT),
                BorderColor::all(LEATHER),
            ))
            .with_children(|card| {
                card.spawn(text(font, *title, TEXT_DOTS, INK));
                for &(key, value) in *lines {
                    card.spawn((
                        Node {
                            align_items: AlignItems::FlexEnd,
                            ..default()
                        },
                        DotStyle {
                            margin_top: 2.0,
                            gap: 3.0,
                            ..default()
                        },
                    ))
                    .with_children(|line| {
                        line.spawn((
                            Node::default(),
                            DotStyle {
                                min_width: Some(14.0),
                                ..default()
                            },
                        ))
                        .with_child(text(font, key, SMALL_DOTS, INK_MUTED));
                        line.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            ..default()
                        })
                        .with_children(|cell| {
                            cell.spawn(text(font, value_text(value), TEXT_DOTS, INK));
                            rule(cell, 0.0);
                        });
                    });
                }
                // 印章
                card.spawn(Node {
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                })
                .with_child((
                    Node::default(),
                    DotStyle {
                        size: Some(Vec2::splat(14.0)),
                        border: 1.0,
                        radius: 7.0,
                        ..default()
                    },
                    BackgroundColor(SEAL),
                    BorderColor::all(LEATHER),
                ));
            });
        }
    }
}

// 下線付きの値の前後に少し余白を空ける
fn value_text(s: &str) -> String {
    format!(" {s} ")
}

pub fn spawn_books(mut commands: Commands, font: Res<UiFont>) {
    let font = &font.0;
    let mut backgrounds: Vec<&'static str> = BOOK_PROPS.iter().map(|b| b.background).collect();
    backgrounds.dedup();

    for background in backgrounds {
        commands
            .spawn((
                BookRoot(background),
                StageNode,
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                // チャット欄より上、トランジションや Fin 画面より下
                GlobalZIndex(12),
                Visibility::Hidden,
            ))
            .with_children(|root| {
                for (i, prop) in BOOK_PROPS.iter().enumerate() {
                    if prop.background != background {
                        continue;
                    }
                    // 本棚上の置き場所 (本を戻すドロップ先も兼ねる)
                    root.spawn((
                        ShelfSpot(i),
                        dot_node(prop.shelf),
                        DotStyle {
                            border: 1.0,
                            radius: 1.0,
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(Color::NONE),
                    ));
                    // 机の上の置き場所
                    root.spawn((
                        Dropzone(i),
                        dot_node(prop.table),
                        DotStyle {
                            border: 1.0,
                            radius: 2.0,
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        BorderColor::all(Color::NONE),
                    ));
                    spawn_book_panel(root, font, i, prop);
                }

                root.spawn((
                    DragGhost,
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    DotStyle {
                        size: Some(Vec2::new(13.0, 17.0)),
                        border: 1.0,
                        radius: 1.0,
                        ..default()
                    },
                    BackgroundColor(PAPER_LIGHT),
                    BorderColor::all(LEATHER),
                    GlobalZIndex(30),
                    Visibility::Hidden,
                ));
            });
    }
}

/// 机に広げた本 (上下の軸 + スクロールする本文 + ×)
fn spawn_book_panel(
    root: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    i: usize,
    prop: &BookProp,
) {
    let rod = |top: bool| {
        (
            BookRod { book: i, top },
            Node {
                width: percent(100),
                flex_shrink: 0.0,
                ..default()
            },
            DotStyle {
                height: Some(if top { ROD_MAX } else { ROD_MIN }),
                border: 1.0,
                radius: 1.0,
                ..default()
            },
            BackgroundColor(ROD),
            BorderColor::all(LEATHER),
        )
    };

    root.spawn((
        BookPanel(i),
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        DotStyle {
            padding: BOOK_PAD,
            border: 1.0,
            radius: 3.0,
            ..default()
        },
        BackgroundColor(PAPER),
        BorderColor::all(LEATHER),
        GlobalZIndex(20),
        Visibility::Hidden,
    ))
    .with_children(|book| {
        book.spawn(rod(true));
        book.spawn((
            BookScroll(i),
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            DotStyle {
                padding: Vec2::new(0.0, 4.0),
                ..default()
            },
            ScrollPosition::default(),
        ))
        .with_children(|scroll| {
            scroll
                .spawn((
                    BookList,
                    Node {
                        flex_direction: FlexDirection::Column,
                        width: percent(100),
                        flex_shrink: 0.0,
                        ..default()
                    },
                ))
                .with_children(|list| {
                    for block in prop.blocks {
                        spawn_block(list, font, block);
                    }
                });
        });
        book.spawn(rod(false));

        // × (本棚に戻す)
        book.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: percent(CLOSE_INSET / BOOK_SIZE.x * 100.0),
                top: percent(CLOSE_INSET / BOOK_SIZE.y * 100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            DotStyle {
                size: Some(Vec2::splat(CLOSE_SIZE)),
                border: 1.0,
                radius: 2.0,
                ..default()
            },
            BackgroundColor(PAPER_LIGHT),
            BorderColor::all(LEATHER),
        ))
        .with_child(text(font, "×", TEXT_DOTS, LEATHER));
    });
}
