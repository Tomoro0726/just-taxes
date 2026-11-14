//! 背景の本棚から資料の巻物を取り出し、机の上で広げて読む。
//!
//! - 本棚の背表紙をクリック → 机の中央に広げる / 広げた巻物の × → 本棚に戻す
//! - 背表紙を机へドラッグ → ドロップした位置に広げる
//! - 広げた巻物をドラッグ → 机の上の好きな位置へ動かす / 本棚へドラッグで戻す
//! - Esc → 広げている巻物をすべて本棚に戻す
//!
//! 中身は `assets/books/<名前>.md` に Markdown で書く (書式は markdown.rs)。
//! 窓口に来た NPC の持ち物の書類 (npc.rs) も同じ巻物として、シーンの間ずっと机の上に広げておく。
//! 座標はすべて背景のドット座標系 (480×270)。

use bevy::asset::io::file::FileAssetReader;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::text::{LineBreak, LineHeight};
use bevy::window::PrimaryWindow;

use crate::markdown::{self, Block};
use crate::panel::{DotFont, DotRect, DotStyle, StageNode, dot_node, rect, stage_transform};
use crate::scenario::Story;
use crate::transition::Transition;
use crate::wrap::{text_em, wrap};
use crate::{DOT_H, DOT_W, UiFont};

pub struct BookProp {
    /// どの背景に属するか
    pub background: &'static str,
    /// 本棚上の置き場所 (背表紙)
    pub shelf: DotRect,
    /// 机の上で巻物を広げられる範囲 (巻物の中心がこの中に収まる)
    pub table: DotRect,
    /// 中身の Markdown (`assets/books/<file>.md`)
    pub file: &'static str,
}

/// 徴税所の机に敷かれた緑のマット
const DESK_MAT: DotRect = rect(138.0, 180.0, 172.0, 86.0);

/// 上段の元帳 4231〜4240 (幅 7 ドットで左から並ぶ)
const fn ledger(no: u32, file: &'static str) -> BookProp {
    BookProp {
        background: "main",
        shelf: rect(263.0 + 7.0 * (no - 4231) as f32, 16.0, 7.0, 39.0),
        table: DESK_MAT,
        file,
    }
}

/// 下段の DICT・MEMO・LAW (幅 13 ドットで左から並ぶ)
const fn lower(index: u32, file: &'static str) -> BookProp {
    BookProp {
        background: "main",
        shelf: rect(263.0 + 13.0 * index as f32, 64.0, 13.0, 39.0),
        table: DESK_MAT,
        file,
    }
}

pub const BOOK_PROPS: &[BookProp] = &[
    ledger(4231, "ledger_4231"),
    ledger(4235, "ledger_4235"),
    ledger(4238, "ledger_4238"),
    lower(0, "dict"),
    lower(1, "memo"),
    lower(2, "tax_law"),
];

// ─── 見た目 ──────────────────────────────────────────────────

/// 広げた巻物の大きさ (ドット)
const BOOK_SIZE: Vec2 = Vec2::new(96.0, 116.0);
/// 巻物がチャット欄 (左下) に重ならないようにする左端
const BOOK_MIN_LEFT: f32 = 128.0;
/// 軸が紙より左右にはみ出す幅
const PAPER_INSET: f32 = 2.0;
const PAPER_PAD: Vec2 = Vec2::new(4.0, 2.0);
/// 本文の幅 (ドット)
const CONTENT_W: f32 = BOOK_SIZE.x - 2.0 * (PAPER_INSET + PAPER_PAD.x);
/// 巻物の軸の太さ (ドット)。スクロール 0 で上が太く下が細い、最後まで読むとその逆
const ROD_MIN: f32 = 3.0;
const ROD_MAX: f32 = 6.0;
const CLOSE_SIZE: f32 = 8.0;
/// DotGothic16 は 16px で崩れずに描けるので、720p (1 ドット = 8/3 px) でちょうど 16px になる 6 ドットにする
const TEXT_DOTS: f32 = 6.0;
const LINE_DOTS: f32 = 8.0;
/// 表の 2 列目以降の最小幅。これより短い値は列が縦にそろう
const VALUE_DOTS: f32 = 24.0;
const CARD_PAD: Vec2 = Vec2::new(3.0, 2.0);
/// クリックとみなす移動量の上限 (論理ピクセル)
const CLICK_THRESHOLD: f32 = 6.0;

const PAPER: Color = Color::srgb_u8(0xe8, 0xd4, 0xb0);
const PAPER_LIGHT: Color = Color::srgb_u8(0xf2, 0xe4, 0xc6);
const ROD: Color = Color::srgb_u8(0xb8, 0x88, 0x66);
const EDGE: Color = Color::srgb_u8(0x6e, 0x48, 0x34);
const INK: Color = Color::srgb_u8(0x2a, 0x22, 0x32);
const INK_MUTED: Color = Color::srgb_u8(0x7a, 0x66, 0x50);
/// 巻物を取り出した後の、空いた本棚の奥の色
const SHELF_EMPTY: Color = Color::srgb_u8(0x34, 0x1c, 0x27);
const HIGHLIGHT: Color = Color::srgba(1.0, 244.0 / 255.0, 210.0 / 255.0, 0.75);
const DROPZONE_FILL: Color = Color::srgba(1.0, 230.0 / 255.0, 160.0 / 255.0, 0.12);

// ─── 状態 ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Place {
    Shelf,
    /// 机の上。値は巻物の左上
    Table(Vec2),
}

struct Drag {
    book: usize,
    /// 机の上の巻物をつかんだときの、巻物の左上からカーソルまでの距離。本棚からなら None
    grab: Option<Vec2>,
    /// 押した位置 (論理ピクセル)。クリックかドラッグかの判定に使う
    start: Vec2,
}

/// 巻物の番号は、本棚の巻物 (BOOK_PROPS の順) のあとに今のシーンの書類が続く
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

    /// 机の上の巻物の左上。ドラッグ中はカーソルに追従する
    fn position(&self, book: usize) -> Option<Vec2> {
        if let (Some(drag), Some(cursor)) = (&self.drag, self.cursor)
            && drag.book == book
            && let Some(grab) = drag.grab
        {
            return Some(clamp_pos(cursor - grab, table_of(book)));
        }
        // シーンが変わった直後は、消える前の書類の巻物がまだ残っている
        match self.places.get(book)? {
            Place::Table(pos) => Some(*pos),
            Place::Shelf => None,
        }
    }

    /// カーソルの下にある、机の上の一番手前の巻物
    fn book_at(&self, p: Vec2) -> Option<usize> {
        self.order
            .iter()
            .rev()
            .copied()
            .find(|&b| self.position(b).is_some_and(|pos| book_rect(pos).contains(p)))
    }
}

/// 巻物や背表紙の上でのクリックは、セリフ送りに使わない
#[derive(Resource, Default)]
pub struct PointerBlocked(pub bool);

fn shelf_of(book: usize) -> Option<DotRect> {
    BOOK_PROPS.get(book).map(|p| p.shelf)
}

fn table_of(book: usize) -> DotRect {
    BOOK_PROPS.get(book).map_or(DESK_MAT, |p| p.table)
}

fn to_rect(r: DotRect) -> Rect {
    Rect::new(r.x, r.y, r.x + r.w, r.y + r.h)
}

fn book_rect(pos: Vec2) -> Rect {
    Rect::from_corners(pos, pos + BOOK_SIZE)
}

fn close_rect(pos: Vec2) -> Rect {
    let min = pos + Vec2::new(BOOK_SIZE.x - PAPER_INSET - 1.0 - CLOSE_SIZE, ROD_MAX + 1.0);
    Rect::from_corners(min, min + Vec2::splat(CLOSE_SIZE))
}

/// 巻物の中心を机の範囲に収め、画面やチャット欄にはみ出さないようにする。
/// ドットの格子からずれると文字がにじむので整数に丸める
fn clamp_pos(pos: Vec2, table: DotRect) -> Vec2 {
    let t = to_rect(table);
    let pos = (pos + BOOK_SIZE / 2.0).clamp(t.min, t.max) - BOOK_SIZE / 2.0;
    Vec2::new(
        pos.x.clamp(BOOK_MIN_LEFT, DOT_W - 2.0 - BOOK_SIZE.x),
        pos.y.clamp(2.0, DOT_H - 2.0 - BOOK_SIZE.y),
    )
    .round()
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
    let shelf_books = BOOK_PROPS.len();

    // 背景が切り替わったら、取り出していた巻物は棚に戻す
    let background = &story.scene().background;
    if books.background != *background {
        books.background.clone_from(background);
        books.places[..shelf_books].fill(Place::Shelf);
        books.order.retain(|&b| b >= shelf_books);
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

    // Escape で読んでいる巻物を棚に戻す
    if keys.just_pressed(KeyCode::Escape) {
        for &b in &in_scene {
            books.places[b] = Place::Shelf;
        }
        books.drag = None;
    }

    let (Some(cursor), Some(cursor_px)) = (cursor, cursor_px) else {
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

    if mouse.just_pressed(MouseButton::Left) {
        if let Some(b) = over_book {
            let pos = books.position(b).unwrap_or_default();
            books.bring_to_front(b);
            if shelf_of(b).is_some() && close_rect(pos).contains(cursor) {
                books.places[b] = Place::Shelf;
            } else {
                books.drag = Some(Drag {
                    book: b,
                    grab: Some(cursor - pos),
                    start: cursor_px,
                });
            }
        } else if let Some(b) = over_shelf {
            books.drag = Some(Drag {
                book: b,
                grab: None,
                start: cursor_px,
            });
        }
    }

    if mouse.just_released(MouseButton::Left)
        && let Some(drag) = books.drag.take()
    {
        let table = table_of(drag.book);
        let on_shelf = shelf_of(drag.book).is_some_and(|s| to_rect(s).contains(cursor));
        let clicked = cursor_px.distance(drag.start) < CLICK_THRESHOLD;
        match drag.grab {
            // 机の上の巻物はクリックでは動かさず、どこでドロップしても机の上に収める
            Some(grab) if !clicked => {
                books.places[drag.book] = if on_shelf {
                    Place::Shelf
                } else {
                    Place::Table(clamp_pos(cursor - grab, table))
                };
            }
            Some(_) => {}
            None => {
                let center = if clicked {
                    Some(to_rect(table).center())
                } else {
                    Some(cursor).filter(|&c| to_rect(table).contains(c))
                };
                if let Some(center) = center {
                    books.places[drag.book] =
                        Place::Table(clamp_pos(center - BOOK_SIZE / 2.0, table));
                    books.bring_to_front(drag.book);
                    // 棚から出したときだけ最初に戻す (机の上で動かしても読んでいた場所は保つ)
                    for (scroll, mut position) in &mut scrolls {
                        if scroll.0 == drag.book {
                            position.0 = Vec2::ZERO;
                        }
                    }
                }
            }
        }
    }
}

/// マウスホイールで広げた巻物を読み進める
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
            MouseScrollUnit::Line => e.y * 3.0 * LINE_DOTS * scale,
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
    let background = &story.scene().background;
    for (root, mut visibility) in &mut roots {
        *visibility = if root.0 == background {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let from_shelf = books.drag.as_ref().filter(|d| d.grab.is_none());

    for (spot, mut fill, mut edge) in &mut spots {
        let on_table = matches!(books.places[spot.0], Place::Table(_));
        // 巻物が机に出ていて棚が空のときは、背表紙を隠す
        fill.0 = if on_table { SHELF_EMPTY } else { Color::NONE };
        let highlight = !on_table && books.hover == Some(spot.0);
        *edge = BorderColor::all(if highlight { HIGHLIGHT } else { Color::NONE });
    }

    // 巻物を置ける机の範囲。普段は見えず、本棚からドラッグ中だけ光る
    for (zone, mut fill, mut edge) in &mut zones {
        let active = from_shelf.is_some_and(|d| d.book == zone.0);
        fill.0 = if active { DROPZONE_FILL } else { Color::NONE };
        *edge = BorderColor::all(if active { HIGHLIGHT } else { Color::NONE });
    }

    for (panel, mut node, mut visibility, mut z) in &mut panels {
        let Some(pos) = books.position(panel.0) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        node.left = percent(pos.x / DOT_W * 100.0);
        node.top = percent(pos.y / DOT_H * 100.0);
        node.width = percent(BOOK_SIZE.x / DOT_W * 100.0);
        node.height = percent(BOOK_SIZE.y / DOT_H * 100.0);
        let depth = books.order.iter().position(|&b| b == panel.0).unwrap_or(0);
        z.0 = 20 + depth as i32;
    }

    // 本棚からドラッグ中のプレビュー
    let (ghost_node, ghost_visibility) = &mut *ghost;
    match (from_shelf, books.cursor) {
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

fn text(font: &Handle<Font>, s: impl Into<String>, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont::from(font.clone()),
        DotFont(TEXT_DOTS),
        LineHeight::RelativeToFont(LINE_DOTS / TEXT_DOTS),
        TextLayout::linebreak(LineBreak::NoWrap),
        TextColor(color),
    )
}

/// 幅 `width` (ドット) に収まるように折り返す
fn wrapped(s: &str, width: f32) -> String {
    wrap(s, width / TEXT_DOTS)
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
        BackgroundColor(EDGE),
    ));
}

/// 表の 1 行。1 列目が値の列と並ばないほど長ければ、値の列を次の行に送る
fn table_row(
    list: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    cells: &[String],
    head: bool,
    width: f32,
) {
    let Some((label, values)) = cells.split_first() else {
        return;
    };
    let color = if head { INK_MUTED } else { INK };
    let values_w: f32 = values
        .iter()
        .map(|v| (text_em(v) * TEXT_DOTS).max(VALUE_DOTS))
        .sum();
    let inline = text_em(label) * TEXT_DOTS + values_w <= width;

    list.spawn((
        Node {
            flex_direction: if inline {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            ..default()
        },
        DotStyle {
            margin_top: 1.0,
            ..default()
        },
    ))
    .with_children(|row| {
        row.spawn(Node {
            flex_grow: 1.0,
            ..default()
        })
        .with_child(text(font, wrapped(label, width), color));
        row.spawn(Node {
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .with_children(|cols| {
            for value in values {
                cols.spawn((
                    Node {
                        justify_content: JustifyContent::FlexEnd,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    DotStyle {
                        min_width: Some(VALUE_DOTS),
                        ..default()
                    },
                ))
                .with_child(text(font, value.as_str(), color));
            }
        });
    });
}

fn spawn_block(list: &mut ChildSpawnerCommands, font: &Handle<Font>, block: &Block, width: f32) {
    let spaced = || {
        (
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            DotStyle {
                margin_top: 4.0,
                ..default()
            },
        )
    };
    match block {
        Block::Heading(s) => {
            list.spawn(spaced()).with_children(|h| {
                h.spawn(text(font, wrapped(s, width), INK));
                rule(h, 1.0);
            });
        }
        Block::Text(s) => {
            list.spawn(spaced())
                .with_child(text(font, wrapped(s, width), INK));
        }
        Block::Rule => rule(list, 3.0),
        Block::Row { cells, head } => table_row(list, font, cells, *head, width),
        Block::Card(blocks) => {
            let (node, mut style) = spaced();
            style.padding = CARD_PAD;
            style.border = 1.0;
            style.radius = 2.0;
            list.spawn((node, style, BackgroundColor(PAPER_LIGHT), BorderColor::all(EDGE)))
                .with_children(|card| {
                    for block in blocks {
                        spawn_block(card, font, block, width - 2.0 * (CARD_PAD.x + 1.0));
                    }
                });
        }
    }
}

fn load_blocks(file: &str) -> Vec<Block> {
    let path = FileAssetReader::get_base_path()
        .join("assets")
        .join("books")
        .join(format!("{file}.md"));
    match std::fs::read_to_string(&path) {
        Ok(md) => markdown::parse(&md),
        Err(e) => vec![Block::Text(format!("{} を読めません ({e})", path.display()))],
    }
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
                    // 本棚上の置き場所 (巻物を戻すドロップ先も兼ねる)
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
                    spawn_book_panel(root, font, i, &load_blocks(prop.file));
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
                    BackgroundColor(PAPER),
                    BorderColor::all(EDGE),
                    GlobalZIndex(30),
                    Visibility::Hidden,
                ));
            });
    }
}

#[derive(Component)]
pub struct DocumentRoot;

/// 今のシーンの NPC の書類を、少しずつずらして机の上に並べる。シーンが変わるたびに作り直す
pub fn sync_documents(
    mut commands: Commands,
    font: Res<UiFont>,
    story: Res<Story>,
    mut books: ResMut<Books>,
    roots: Query<Entity, With<DocumentRoot>>,
    mut shown: Local<Option<usize>>,
) {
    if *shown == Some(story.scene_idx) {
        return;
    }
    *shown = Some(story.scene_idx);

    for root in &roots {
        commands.entity(root).despawn();
    }
    let shelf_books = BOOK_PROPS.len();
    books.places.resize(shelf_books, Place::Shelf);
    books.order.retain(|&b| b < shelf_books);
    books.drag = None;

    let documents = &story.scene().documents;
    let start = to_rect(DESK_MAT).center() - BOOK_SIZE / 2.0;
    let spread = documents.len().saturating_sub(1) as f32 / 2.0;
    for j in 0..documents.len() {
        let offset = Vec2::new(18.0, 6.0) * (j as f32 - spread);
        books
            .places
            .push(Place::Table(clamp_pos(start + offset, DESK_MAT)));
        books.order.push(shelf_books + j);
    }

    commands
        .spawn((
            DocumentRoot,
            StageNode,
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(12),
        ))
        .with_children(|root| {
            for (j, blocks) in documents.iter().enumerate() {
                spawn_book_panel(root, &font.0, shelf_books + j, blocks);
            }
        });
}

/// 机に広げた巻物 (上下の軸 + スクロールする紙。本棚の巻物には × も付ける)
fn spawn_book_panel(
    root: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    i: usize,
    blocks: &[Block],
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
            BorderColor::all(EDGE),
        )
    };

    root.spawn((
        BookPanel(i),
        Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        GlobalZIndex(20),
        Visibility::Hidden,
    ))
    .with_children(|book| {
        book.spawn(rod(true));
        // 紙 (DotStyle は margin を上書きするので、左右の余白は紙側、内側の余白はスクロール側に持たせる)
        book.spawn((
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                margin: UiRect::horizontal(percent(PAPER_INSET / BOOK_SIZE.x * 100.0)),
                ..default()
            },
            BackgroundColor(PAPER),
        ))
        .with_children(|paper| {
            paper
                .spawn((
                    BookScroll(i),
                    Node {
                        flex_grow: 1.0,
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                        // 既定の Stretch だとリストの高さが表示領域に固定され、スクロール量が 0 になる
                        align_items: AlignItems::FlexStart,
                        ..default()
                    },
                    DotStyle {
                        padding: PAPER_PAD,
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
                            for block in blocks {
                                spawn_block(list, font, block, CONTENT_W);
                            }
                        });
                });
        });
        book.spawn(rod(false));

        // × (本棚に戻す)
        if shelf_of(i).is_none() {
            return;
        }
        book.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: percent((PAPER_INSET + 1.0) / BOOK_SIZE.x * 100.0),
                top: percent((ROD_MAX + 1.0) / BOOK_SIZE.y * 100.0),
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
            BorderColor::all(EDGE),
        ))
        .with_child(text(font, "×", EDGE));
    });
}
