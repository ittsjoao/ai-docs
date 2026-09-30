use std::path::Path;

use anyhow::Context;
use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_hollow_circle_mut, draw_hollow_rect_mut};
use screenmanual_core::domain::{CropSpec, Mark, Rect};
use screenmanual_core::ports::{Imaging, PortResult};

const RED: Rgba<u8> = Rgba([220, 38, 38, 255]);
const STROKE: i32 = 3;

/// Recortes dos prints com destaque vermelho (spec §6.2).
pub struct ImageCrops;

impl Imaging for ImageCrops {
    fn render_crops(&self, dir: &Path, specs: &[CropSpec]) -> PortResult<()> {
        for spec in specs {
            let source = dir.join(&spec.source);
            if !source.is_file() {
                // ponytail: a captura pode perder um PNG (R9 do plano 02); o passo fica sem imagem
                eprintln!("print ausente, recorte pulado: {}", spec.source);
                continue;
            }
            let shot = image::open(&source)
                .with_context(|| format!("falha ao abrir {}", spec.source))?
                .to_rgba8();
            let out = dir.join(&spec.out);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            render(&shot, spec)
                .save(&out)
                .with_context(|| format!("falha ao gravar {}", spec.out))?;
        }
        Ok(())
    }
}

/// Recorta `spec.region` (pixels de tela) do print de `spec.monitor`, desenha a marca
/// e reduz para o lado maior caber em `spec.max_side`.
pub(crate) fn render(shot: &RgbaImage, spec: &CropSpec) -> RgbaImage {
    let (w, h) = (shot.width() as i32, shot.height() as i32);
    let (ox, oy) = (spec.monitor.left, spec.monitor.top);
    let (mut x0, mut y0) = (
        (spec.region.left - ox).clamp(0, w),
        (spec.region.top - oy).clamp(0, h),
    );
    let (mut x1, mut y1) = (
        (spec.region.right - ox).clamp(x0, w),
        (spec.region.bottom - oy).clamp(y0, h),
    );
    if x1 <= x0 || y1 <= y0 {
        (x0, y0, x1, y1) = (0, 0, w, h); // região fora do print: usa o print inteiro
    }
    let mut crop = imageops::crop_imm(
        shot,
        x0 as u32,
        y0 as u32,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    )
    .to_image();
    let (dx, dy) = (ox + x0, oy + y0);
    match &spec.mark {
        Mark::Rect { rect } => draw_rect(&mut crop, *rect, dx, dy),
        Mark::Circle { x, y, r } => {
            for i in 0..STROKE {
                draw_hollow_circle_mut(&mut crop, (x - dx, y - dy), r + i, RED);
            }
        }
    }
    let side = crop.width().max(crop.height());
    if side <= spec.max_side {
        return crop;
    }
    let k = spec.max_side as f32 / side as f32;
    let (nw, nh) = (
        ((crop.width() as f32 * k).round() as u32).max(1),
        ((crop.height() as f32 * k).round() as u32).max(1),
    );
    imageops::resize(&crop, nw, nh, FilterType::Triangle)
}

fn draw_rect(img: &mut RgbaImage, r: Rect, dx: i32, dy: i32) {
    for i in 0..STROKE {
        let (l, t, rr, b) = (
            r.left - dx - i,
            r.top - dy - i,
            r.right - dx + i,
            r.bottom - dy + i,
        );
        if rr > l && b > t {
            draw_hollow_rect_mut(
                img,
                imageproc::rect::Rect::at(l, t).of_size((rr - l) as u32, (b - t) as u32),
                RED,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn spec(monitor: Rect, region: Rect, mark: Mark, max_side: u32) -> CropSpec {
        CropSpec {
            source: "shots/a.png".into(),
            monitor,
            region,
            mark,
            max_side,
            out: "crops/c001.png".into(),
        }
    }

    #[test]
    fn crops_in_screen_coordinates_of_a_monitor_left_of_the_primary() {
        let shot = RgbaImage::from_pixel(200, 100, WHITE);
        let s = spec(
            rect(-200, 0, 0, 100),
            rect(-150, 10, -50, 60),
            Mark::Rect {
                rect: rect(-120, 20, -80, 40),
            },
            1280,
        );
        let out = render(&shot, &s);
        assert_eq!(out.dimensions(), (100, 50));
        assert_eq!(
            *out.get_pixel(30, 10),
            RED,
            "canto da marca em coordenadas do recorte"
        );
        assert_eq!(*out.get_pixel(50, 20), WHITE, "retângulo é vazado");
    }

    #[test]
    fn circle_mark_is_drawn_around_the_click() {
        let shot = RgbaImage::from_pixel(200, 100, WHITE);
        let out = render(
            &shot,
            &spec(
                rect(0, 0, 200, 100),
                rect(0, 0, 200, 100),
                Mark::Circle {
                    x: 100,
                    y: 50,
                    r: 18,
                },
                1280,
            ),
        );
        assert_eq!(*out.get_pixel(118, 50), RED);
        assert_eq!(*out.get_pixel(100, 50), WHITE);
    }

    #[test]
    fn largest_side_is_reduced_to_max_side() {
        let shot = RgbaImage::from_pixel(400, 200, WHITE);
        let out = render(
            &shot,
            &spec(
                rect(0, 0, 400, 200),
                rect(0, 0, 400, 200),
                Mark::Circle {
                    x: 200,
                    y: 100,
                    r: 18,
                },
                100,
            ),
        );
        assert_eq!(out.dimensions(), (100, 50));
    }

    #[test]
    fn missing_shot_is_skipped_and_others_are_written() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("smimg-{nanos}"));
        std::fs::create_dir_all(dir.join("shots")).unwrap();
        RgbaImage::from_pixel(200, 100, WHITE)
            .save(dir.join("shots").join("a.png"))
            .unwrap();
        let ok = spec(
            rect(0, 0, 200, 100),
            rect(0, 0, 100, 50),
            Mark::Circle {
                x: 50,
                y: 25,
                r: 18,
            },
            1280,
        );
        let mut lost = ok.clone();
        lost.source = "shots/sumiu.png".into();
        lost.out = "crops/c002.png".into();
        ImageCrops.render_crops(&dir, &[ok, lost]).unwrap();
        assert_eq!(
            image::open(dir.join("crops").join("c001.png"))
                .unwrap()
                .width(),
            100
        );
        assert!(!dir.join("crops").join("c002.png").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
