//! CSS box-shadow 形式のドット絵をそのまま読み込むアセットローダー。
//!
//! `assets/scenes/<name>.css` を置くと、PNG に変換せずに
//! 1 ドット = 1 テクセルのテクスチャとして描画できる。
//!
//! 対応フォーマット (1 ドット 1 行):
//!   calc(X*var(--shadow-mult)) calc(Y*var(--shadow-mult)) var(--blur) var(--spread) #RRGGBB[,;]

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext, RenderAssetUsages};
use bevy::image::{ImageSampler, TextureFormatPixelInfo};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub struct DotCssPlugin;

impl Plugin for DotCssPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(DotCssLoader);
    }
}

#[derive(TypePath)]
struct DotCssLoader;

impl AssetLoader for DotCssLoader {
    type Asset = Image;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Image, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let css = std::str::from_utf8(&bytes)?;
        let image = parse_dot_css(css)
            .map_err(|e| format!("{}: {e}", load_context.path().path().display()))?;
        Ok(image)
    }

    fn extensions(&self) -> &[&str] {
        &["css"]
    }
}

struct Dot {
    x: u32,
    y: u32,
    rgb: [u8; 3],
}

/// "calc(123*" の数値部分を読む
fn parse_calc(s: &str) -> Option<(u32, &str)> {
    let s = s.trim_start().strip_prefix("calc(")?;
    let (num, rest) = s.split_once('*')?;
    Some((num.trim().parse().ok()?, rest))
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let hex: String = s.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    let digit = |i: usize, len: usize| u8::from_str_radix(&hex[i..i + len], 16).ok();
    match hex.len() {
        // #RGB → #RRGGBB
        3 => Some([digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17]),
        // #RRGGBBAA はアルファを無視する
        6 | 8 => Some([digit(0, 2)?, digit(2, 2)?, digit(4, 2)?]),
        _ => None,
    }
}

fn parse_dot(line: &str) -> Option<Dot> {
    let (x, rest) = parse_calc(line)?;
    let (y, rest) = parse_calc(&rest[rest.find("calc(")?..])?;
    let (_, color) = rest.rsplit_once('#')?;
    Some(Dot {
        x,
        y,
        rgb: parse_hex(color)?,
    })
}

fn parse_dot_css(css: &str) -> Result<Image, String> {
    let dots: Vec<Dot> = css.lines().filter_map(parse_dot).collect();
    if dots.is_empty() {
        return Err(
            "ピクセルデータが見つかりませんでした。CSS のフォーマットを確認してください。".into(),
        );
    }

    let width = dots.iter().map(|d| d.x).max().unwrap_or(0) + 1;
    let height = dots.iter().map(|d| d.y).max().unwrap_or(0) + 1;

    // 指定のないドットは透明
    let format = TextureFormat::Rgba8UnormSrgb;
    let stride = format.pixel_size().map_err(|e| e.to_string())?;
    let mut data = vec![0u8; (width * height) as usize * stride];
    for dot in &dots {
        let i = (dot.y * width + dot.x) as usize * stride;
        data[i..i + 4].copy_from_slice(&[dot.rgb[0], dot.rgb[1], dot.rgb[2], 255]);
    }

    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    // ドットをぼかさずに拡大する
    image.sampler = ImageSampler::nearest();
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_box_shadow_lines() {
        let css = "\
            .pixel-art {\n\
            box-shadow:\n\
            calc(0*var(--shadow-mult)) calc(0*var(--shadow-mult)) var(--blur) var(--spread) #341C27,\n\
            calc(2*var(--shadow-mult)) calc(1*var(--shadow-mult)) var(--blur) var(--spread) #fff;\n\
            }";
        let image = parse_dot_css(css).unwrap();
        assert_eq!(image.width(), 3);
        assert_eq!(image.height(), 2);
        let data = image.data.as_ref().unwrap();
        assert_eq!(&data[0..4], &[0x34, 0x1c, 0x27, 255]);
        // (1, 0) は指定がないので透明
        assert_eq!(&data[4..8], &[0, 0, 0, 0]);
        let i = (3 + 2) * 4;
        assert_eq!(&data[i..i + 4], &[255, 255, 255, 255]);
    }
}
