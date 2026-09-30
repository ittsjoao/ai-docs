use super::Draft;
use crate::domain::{BuildConfig, CropSpec, Mark, Quality, Rect};

const CIRCLE_RADIUS: i32 = 18;

pub(crate) fn fit(center: (i32, i32), w: i32, h: i32, bounds: Rect) -> Rect {
    let w = w.min(bounds.width());
    let h = h.min(bounds.height());
    let left = (center.0 - w / 2).clamp(bounds.left, bounds.right - w);
    let top = (center.1 - h / 2).clamp(bounds.top, bounds.bottom - h);
    Rect {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}

pub(crate) fn crop_specs(drafts: &mut [Draft], cfg: &BuildConfig) -> Vec<CropSpec> {
    let mut specs = Vec::new();
    for d in drafts.iter_mut() {
        let (Some(shot), Some(monitor)) = (d.shot.clone(), d.monitor) else {
            continue;
        };
        let uia_rect = d
            .cand
            .el
            .as_ref()
            .filter(|e| e.quality == Quality::Uia)
            .and_then(|e| e.rect);
        let (region, mark) = match uia_rect {
            Some(r) => {
                let w = (r.width() + 2 * cfg.crop_pad).max(cfg.min_crop.0);
                let h = (r.height() + 2 * cfg.crop_pad).max(cfg.min_crop.1);
                (fit(r.center(), w, h, monitor), Mark::Rect { rect: r })
            }
            None => (
                fit(d.point, cfg.fixed_crop.0, cfg.fixed_crop.1, monitor),
                Mark::Circle {
                    x: d.point.0,
                    y: d.point.1,
                    r: CIRCLE_RADIUS,
                },
            ),
        };
        let out = format!("crops/{}.png", d.cand.id);
        specs.push(CropSpec {
            source: shot.clone(),
            monitor,
            region,
            mark: mark.clone(),
            max_side: cfg.max_side,
            out: out.clone(),
        });
        d.cand.crop = Some(out);
        if d.first_in_window {
            let ctx = format!("crops/{}_ctx.png", d.cand.id);
            specs.push(CropSpec {
                source: shot,
                monitor,
                region: monitor,
                mark,
                max_side: cfg.max_side,
                out: ctx.clone(),
            });
            d.cand.context_shot = Some(ctx);
        }
    }
    specs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::*;
    use crate::pipeline::build::build;
    use crate::pipeline::build::group::group;
    use crate::pipeline::build::test_util::*;

    #[test]
    fn fit_clamps_inside_monitor() {
        let r = fit((10, 10), 640, 400, mon());
        assert_eq!(
            r,
            Rect {
                left: 0,
                top: 0,
                right: 640,
                bottom: 400
            }
        );
        let r = fit((1910, 1070), 640, 400, mon());
        assert_eq!(
            r,
            Rect {
                left: 1280,
                top: 680,
                right: 1920,
                bottom: 1080
            }
        );
        let r = fit((100, 100), 5000, 5000, mon());
        assert_eq!(r, mon());
    }

    #[test]
    fn uia_element_gets_padded_rect_and_rect_mark() {
        let cfg = BuildConfig::default();
        let mut d = group(&[click(1000, 200, 115, Some(field("CNPJ")))], &cfg);
        let specs = crop_specs(&mut d, &cfg);
        let e = Rect {
            left: 100,
            top: 100,
            right: 300,
            bottom: 130,
        };
        assert_eq!(specs[0].mark, Mark::Rect { rect: e });
        assert_eq!(
            (specs[0].region.width(), specs[0].region.height()),
            (480, 300)
        );
        assert_eq!(specs[0].out, "crops/c001.png");
        assert_eq!(d[0].cand.crop.as_deref(), Some("crops/c001.png"));
    }

    #[test]
    fn generic_element_gets_fixed_box_and_circle() {
        let cfg = BuildConfig::default();
        let mut d = group(
            &[click(
                1000,
                900,
                500,
                Some(el("", "Pane", Quality::Generic, None)),
            )],
            &cfg,
        );
        let specs = crop_specs(&mut d, &cfg);
        assert_eq!(
            specs[0].mark,
            Mark::Circle {
                x: 900,
                y: 500,
                r: 18
            }
        );
        assert_eq!(
            (specs[0].region.width(), specs[0].region.height()),
            (640, 400)
        );
    }

    #[test]
    fn first_click_in_window_also_gets_context_shot() {
        let cfg = BuildConfig::default();
        let mut d = group(
            &[
                win(0, "ERP"),
                click(1000, 1, 1, None),
                click(3000, 900, 900, None),
            ],
            &cfg,
        );
        let specs = crop_specs(&mut d, &cfg);
        assert_eq!(specs.len(), 3);
        assert_eq!(
            d[1].cand.context_shot.as_deref(),
            Some("crops/c002_ctx.png")
        );
        assert_eq!(specs[1].region, mon());
        assert!(d[0].cand.crop.is_none(), "switch não tem print");
    }

    #[test]
    fn build_runs_all_stages() {
        let out = build(
            &[
                win(0, "ERP"),
                click(1000, 200, 115, Some(field("CNPJ"))),
                typ(2000, field("CNPJ"), 14),
                key(2100, "Enter"),
            ],
            &[seg(500, 1800, "aqui coloca o CNPJ")],
            &BuildConfig::default(),
        );
        assert_eq!(out.candidates.len(), 2);
        let fill = &out.candidates[1];
        assert_eq!(fill.kind, CandidateKind::Fill);
        assert_eq!(fill.speech, vec!["aqui coloca o CNPJ"]);
        assert_eq!(out.crops.len(), 2, "crop + contexto");
    }
}
