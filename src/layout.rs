//! Window placement inside a workspace card.
//!
//! The default "wrap" mode reflows a workspace's windows into a wrapping grid
//! so they fill the card instead of reproducing the compositor's geometry (a
//! single row of tall tiles would otherwise render as a thin strip). The
//! legacy geometry mode is kept behind `appearance.wrap_windows = false`.
//!
//! Rendering and pointer hit-testing both call [`window_rects`], so the drawn
//! thumbnail and the clickable area can never drift apart.

use crate::ipc::ClientInfo;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// Number of columns in the workspace grid.
///
/// `configured == 0` means automatic: a square grid (`ceil(sqrt(n))`). A
/// non-zero value is an upper bound rather than a fixed width, so a handful of
/// workspaces still arrange in a square while large counts stop growing.
pub fn workspace_grid_cols(workspaces: usize, configured: usize) -> usize {
    let auto = ((workspaces as f64).sqrt().ceil() as usize).max(1);
    match configured {
        0 => auto,
        max => auto.min(max).max(1),
    }
}

/// Final on-screen rectangles for `clients` inside `area`, in client order.
///
/// `gap` is the visual spacing between adjacent thumbnails and `wrap` selects
/// between the reflowing grid and the scaled compositor geometry.
pub fn window_rects(clients: &[ClientInfo], area: Rect, gap: f64, wrap: bool) -> Vec<Rect> {
    if clients.is_empty() || area.w <= 0.0 || area.h <= 0.0 {
        return Vec::new();
    }
    if wrap {
        wrapped_rects(clients, area, gap)
    } else {
        geometric_rects(clients, area, gap)
    }
}

/// Reflow windows into a uniform grid. Candidate column counts are scored by
/// how much of the card their aspect-fitted thumbnails cover, which naturally
/// picks a balanced 2x2 for four landscape windows rather than 1x4 or 4x1.
fn wrapped_rects(clients: &[ClientInfo], area: Rect, gap: f64) -> Vec<Rect> {
    let n = clients.len();
    let mut cols = 1;
    let mut best_used = f64::NEG_INFINITY;

    for candidate in 1..=n {
        let rows = n.div_ceil(candidate);
        let cell_w = (area.w - gap * candidate.saturating_sub(1) as f64) / candidate as f64;
        let cell_h = (area.h - gap * rows.saturating_sub(1) as f64) / rows as f64;
        if cell_w <= 0.0 || cell_h <= 0.0 {
            continue;
        }
        let used: f64 = clients
            .iter()
            .map(|c| {
                let cw = c.w.max(1) as f64;
                let ch = c.h.max(1) as f64;
                let scale = (cell_w / cw).min(cell_h / ch);
                cw * ch * scale * scale
            })
            .sum();
        if used > best_used {
            best_used = used;
            cols = candidate;
        }
    }

    let rows = n.div_ceil(cols);
    let cell_w = ((area.w - gap * cols.saturating_sub(1) as f64) / cols as f64).max(1.0);
    let cell_h = ((area.h - gap * rows.saturating_sub(1) as f64) / rows as f64).max(1.0);

    // Reflow in reading order (top-to-bottom, left-to-right in the compositor's
    // layout) so the visual order matches Tab cycling, then map back to the
    // caller's client order.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (clients[i].y, clients[i].x));

    let mut rects = vec![
        Rect {
            x: area.x,
            y: area.y,
            w: 1.0,
            h: 1.0,
        };
        n
    ];
    for (position, &index) in order.iter().enumerate() {
        let row = position / cols;
        let col = position % cols;
        let in_row = (n - row * cols).min(cols);
        let row_w = in_row as f64 * cell_w + in_row.saturating_sub(1) as f64 * gap;
        let row_x = area.x + (area.w - row_w) / 2.0;
        let cell_x = row_x + col as f64 * (cell_w + gap);
        let cell_y = area.y + row as f64 * (cell_h + gap);

        let c = &clients[index];
        let cw = c.w.max(1) as f64;
        let ch = c.h.max(1) as f64;
        let scale = (cell_w / cw).min(cell_h / ch);
        let w = (cw * scale).max(1.0);
        let h = (ch * scale).max(1.0);
        rects[index] = Rect {
            x: cell_x + (cell_w - w) / 2.0,
            y: cell_y + (cell_h - h) / 2.0,
            w,
            h,
        };
    }
    rects
}

/// Legacy layout: scale the compositor's rectangles to fit the card, keeping
/// their relative positions and sizes.
fn geometric_rects(clients: &[ClientInfo], area: Rect, gap: f64) -> Vec<Rect> {
    let (min_x, min_y, max_x, max_y) = clients.iter().fold(
        (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
        |(mix, miy, mxx, mxy), c| {
            (
                mix.min(c.x),
                miy.min(c.y),
                mxx.max(c.x + c.w),
                mxy.max(c.y + c.h),
            )
        },
    );

    let ws_w = (max_x - min_x).max(1) as f64;
    let ws_h = (max_y - min_y).max(1) as f64;
    let scale = (area.w / ws_w).min(area.h / ws_h) * 0.9;
    let off_x = area.x + (area.w - ws_w * scale) / 2.0;
    let off_y = area.y + (area.h - ws_h * scale) / 2.0;
    // Inset by half the gap on each side so abutting clients end up one full
    // gap apart.
    let inset = gap / 2.0;

    clients
        .iter()
        .map(|c| Rect {
            x: off_x + (c.x - min_x) as f64 * scale + inset,
            y: off_y + (c.y - min_y) as f64 * scale + inset,
            w: (c.w as f64 * scale - gap).max(1.0),
            h: (c.h as f64 * scale - gap).max(1.0),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(x: i32, y: i32, w: i32, h: i32) -> ClientInfo {
        ClientInfo {
            address: (x * 1000 + y) as u64,
            class_name: "foot".into(),
            title: "term".into(),
            initial_title: String::new(),
            workspace_id: 1,
            x,
            y,
            w,
            h,
        }
    }

    fn area() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            w: 464.0,
            h: 280.0,
        }
    }

    #[test]
    fn grid_cols_uses_a_square_when_unconfigured() {
        assert_eq!(workspace_grid_cols(1, 0), 1);
        assert_eq!(workspace_grid_cols(4, 0), 2);
        assert_eq!(workspace_grid_cols(9, 0), 3);
        assert_eq!(workspace_grid_cols(10, 0), 4);
    }

    #[test]
    fn grid_cols_caps_the_automatic_layout() {
        // n=3 wants two columns; a cap of 4 must not force three in one row.
        assert_eq!(workspace_grid_cols(3, 4), 2);
        // n=9 wants three; a cap of 2 reflows into more rows.
        assert_eq!(workspace_grid_cols(9, 2), 2);
        // n=16 wants four, exactly the cap.
        assert_eq!(workspace_grid_cols(16, 4), 4);
        // A cap of one always means a single column.
        assert_eq!(workspace_grid_cols(9, 1), 1);
    }

    #[test]
    fn grid_cols_never_returns_zero() {
        assert_eq!(workspace_grid_cols(0, 0), 1);
        assert_eq!(workspace_grid_cols(0, 4), 1);
    }

    #[test]
    fn wrap_balances_four_landscape_windows_into_two_rows() {
        let clients = vec![
            client(0, 0, 800, 600),
            client(800, 0, 800, 600),
            client(0, 600, 800, 600),
            client(800, 600, 800, 600),
        ];

        let rects = window_rects(&clients, area(), 4.0, true);

        assert_eq!(rects.len(), 4);
        // Two columns: the first pair shares a row, the second pair starts below.
        assert_eq!(rects[0].y, rects[1].y);
        assert_eq!(rects[2].y, rects[3].y);
        assert!(rects[2].y > rects[0].y);
        assert!(rects[1].x > rects[0].x);
    }

    #[test]
    fn wrap_preserves_each_window_aspect_ratio() {
        let clients = vec![
            client(0, 0, 1600, 900),
            client(1600, 0, 900, 1600),
            client(0, 900, 1000, 1000),
        ];

        let rects = window_rects(&clients, area(), 4.0, true);

        for (c, r) in clients.iter().zip(&rects) {
            let expected = c.w as f64 / c.h as f64;
            let actual = r.w / r.h;
            assert!(
                (actual - expected).abs() < 0.01,
                "aspect {actual} should match {expected}"
            );
        }
    }

    #[test]
    fn wrap_keeps_thumbnails_inside_the_card() {
        let clients: Vec<_> = (0..9)
            .map(|i| client((i % 3) * 400, (i / 3) * 300, 400, 300))
            .collect();

        let a = area();
        for r in window_rects(&clients, a, 4.0, true) {
            assert!(r.x >= a.x - 0.001, "{r:?} starts left of the card");
            assert!(r.y >= a.y - 0.001, "{r:?} starts above the card");
            assert!(r.x + r.w <= a.x + a.w + 0.001, "{r:?} overflows right");
            assert!(r.y + r.h <= a.y + a.h + 0.001, "{r:?} overflows bottom");
        }
    }

    #[test]
    fn wrap_centers_a_trailing_odd_window() {
        let clients = vec![
            client(0, 0, 800, 600),
            client(800, 0, 800, 600),
            client(0, 600, 800, 600),
        ];

        let a = area();
        let rects = window_rects(&clients, a, 4.0, true);

        // Three landscape windows pick two columns, so the last one sits alone
        // on the second row and should be centered rather than left-aligned.
        assert!(rects[2].y > rects[0].y);
        let center = rects[2].x + rects[2].w / 2.0;
        assert!((center - (a.x + a.w / 2.0)).abs() < 0.01);
    }

    #[test]
    fn wrap_uses_reading_order_for_reflow() {
        // `clients` is deliberately out of visual order; the top-right window
        // should occupy the first grid cell.
        let mut top_right = client(900, 0, 400, 400);
        top_right.address = 2;
        let mut bottom_left = client(0, 700, 400, 400);
        bottom_left.address = 3;
        let clients = vec![bottom_left, top_right];

        let rects = window_rects(&clients, area(), 4.0, true);

        // Two windows fit in one row; reading order puts top-right (index 1)
        // at the left and bottom-left (index 0) at the right.
        assert!(rects[1].x < rects[0].x);
    }

    #[test]
    fn wrap_handles_empty_clients_and_degenerate_area() {
        assert!(window_rects(&[], area(), 4.0, true).is_empty());
        let clients = vec![client(0, 0, 800, 600)];
        let zero = Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
        assert!(window_rects(&clients, zero, 4.0, true).is_empty());
    }

    #[test]
    fn geometry_mode_preserves_the_compositor_layout() {
        let clients = vec![client(0, 0, 800, 600), client(800, 0, 800, 600)];

        let rects = window_rects(&clients, area(), 4.0, false);

        // Side-by-side windows stay side by side and share a row.
        assert_eq!(rects[0].y, rects[1].y);
        assert!(rects[1].x > rects[0].x);
        // The visual gap between neighbours is the configured window gap.
        assert!((rects[1].x - (rects[0].x + rects[0].w) - 4.0).abs() < 0.01);
    }

    #[test]
    fn geometry_mode_puts_a_tall_column_in_one_visual_row() {
        let clients = vec![
            client(0, 0, 300, 1000),
            client(300, 0, 300, 1000),
            client(600, 0, 300, 1000),
        ];

        let rects = window_rects(&clients, area(), 4.0, false);

        for r in &rects {
            assert!((r.y - rects[0].y).abs() < 0.01);
        }
    }
}
