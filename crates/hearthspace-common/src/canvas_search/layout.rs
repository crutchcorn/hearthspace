use super::{Field, Layout, Placement, Point, Rect};

struct Grid {
    columns: usize,
    column_gap: f64,
    row_gap: f64,
    cell_width: f64,
    cell_height: f64,
    width: f64,
    height: f64,
    score: f64,
}

fn fit_grid(
    count: usize,
    available_width: f64,
    available_height: f64,
    large: bool,
    caption_space: f64,
) -> Grid {
    let initial = ((available_width + 18.0) / if large { 258.0 } else { 114.0 })
        .floor()
        .max(1.0) as usize;
    let initial = initial.min(count.max(1));
    let desired_width: f64 = if large { 320.0 } else { 96.0 };
    let desired_height: f64 = if large { 216.0 } else { 104.0 };
    let fit = |columns: usize| {
        let rows = count.div_ceil(columns).max(1) as f64;
        let cols = columns as f64;
        let column_gap = 18.0_f64.min(available_width / (cols * 4.0));
        let row_gap = caption_space.max(18.0_f64.min(available_height / (rows * 4.0)));
        let cell_width = desired_width.min((available_width - column_gap * (cols - 1.0)) / cols);
        let cell_height = desired_height
            .min((available_height - row_gap * (rows - 1.0) - caption_space) / rows)
            .max(0.0);
        Grid {
            columns,
            column_gap,
            row_gap,
            cell_width,
            cell_height,
            width: cols * cell_width + (cols - 1.0) * column_gap,
            height: rows * cell_height + (rows - 1.0) * row_gap + caption_space,
            score: (cell_width / desired_width).min(cell_height / desired_height),
        }
    };
    let mut best = fit(initial);
    if large {
        for columns in initial + 1..=count {
            let candidate = fit(columns);
            if candidate.score > best.score {
                best = candidate;
            }
        }
    }
    best
}

/// Reference spatial layout, in global screen logical coordinates. `sizes` are
/// the unscaled, visible sizes of matching items, already in result order.
pub fn layout(
    view: Rect,
    anchor: Point,
    sizes: &[(f64, f64)],
    large_results: bool,
    handwriting: bool,
) -> Layout {
    let side = if view.width <= 760.0 {
        12.0_f64
    } else {
        24.0_f64
    }
    .min((view.width - 1.0).max(0.0) / 2.0);
    let vertical = if view.width <= 760.0 {
        78.0_f64
    } else {
        96.0_f64
    }
    .min((view.height - 54.0).max(0.0) / 2.0);
    let width = 420.0_f64.min((view.width - side * 2.0).max(1.0));
    let height = 54.0_f64.min(view.height.max(1.0));
    let handwriting_height = if handwriting {
        145.0_f64.min((view.height - vertical * 2.0 - height - 2.0).max(0.0))
    } else {
        0.0
    };
    let x = (anchor.x - width / 2.0).clamp(
        view.x + side,
        (view.x + view.width - side - width).max(view.x + side),
    );
    let mut y = (anchor.y - height / 2.0).clamp(
        view.y + vertical,
        (view.y + view.height - vertical - height).max(view.y + vertical),
    );
    let results_above = y + height / 2.0 > view.y + view.height / 2.0;
    let min_y = view.y
        + vertical
        + if results_above {
            handwriting_height
        } else {
            0.0
        };
    let max_y = view.y + view.height
        - vertical
        - height
        - if results_above {
            0.0
        } else {
            handwriting_height
        };
    y = y.clamp(min_y, max_y.max(min_y));
    let available_width = (view.width - side * 2.0).max(1.0);
    let available_height = if results_above {
        y - view.y - 24.0 - vertical - handwriting_height
    } else {
        view.y + view.height - vertical - y - height - 24.0 - handwriting_height
    }
    .max(1.0);
    let grid = fit_grid(
        sizes.len(),
        available_width,
        available_height,
        large_results,
        0.0,
    );
    let origin_x = view.x + (view.width - grid.width) / 2.0;
    let origin_y = if results_above {
        y - 24.0 - grid.height - handwriting_height
    } else {
        y + height + 24.0 + handwriting_height
    };
    let placements = place(sizes, &grid, origin_x, origin_y);
    Layout {
        field: Field {
            rect: Rect {
                x,
                y,
                width,
                height,
            },
            results_above,
            handwriting_height,
        },
        placements,
    }
}

fn place(sizes: &[(f64, f64)], grid: &Grid, origin_x: f64, origin_y: f64) -> Vec<Placement> {
    sizes
        .iter()
        .enumerate()
        .map(|(index, &(width, height))| {
            let scale = 1.0_f64
                .min(grid.cell_width / width.max(1.0))
                .min(grid.cell_height / height.max(1.0));
            Placement {
                x: origin_x
                    + (index % grid.columns) as f64 * (grid.cell_width + grid.column_gap)
                    + (grid.cell_width - width * scale) / 2.0,
                y: origin_y
                    + (index / grid.columns) as f64 * (grid.cell_height + grid.row_gap)
                    + (grid.cell_height - height * scale) / 2.0,
                scale,
                matches: true,
            }
        })
        .collect()
}

/// Center the preview grid on both output axes, including the space below each
/// row for the selected title. Sizes include window decorations and stay in the
/// session's MRU order.
pub fn window_switcher_layout(view: Rect, sizes: &[(f64, f64)]) -> Vec<Placement> {
    let compact = view.width <= 760.0;
    let side = if compact { 12.0_f64 } else { 24.0_f64 }.min((view.width - 1.0).max(0.0) / 2.0);
    let vertical =
        if compact { 78.0_f64 } else { 96.0_f64 }.min((view.height - 36.0).max(0.0) / 2.0);
    let available_width = (view.width - side * 2.0).max(1.0);
    let available_height = (view.height - vertical * 2.0).max(1.0);
    let grid = fit_grid(
        sizes.len(),
        available_width,
        available_height,
        true,
        36.0_f64.min(available_height),
    );
    place(
        sizes,
        &grid,
        view.x + (view.width - grid.width) / 2.0,
        view.y + (view.height - grid.height) / 2.0,
    )
}
