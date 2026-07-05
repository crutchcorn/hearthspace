use xilem::dpi::LogicalSize;

/// Maximum number of app search results to surface at once.
pub(super) const MAX_RESULTS: usize = 4;

/// Width of the launcher palette dropdown, in logical pixels.
pub(super) const LAUNCHER_WIDTH: i32 = 360;
/// Approximate height of a single result row, used to size the dropdown to its
/// contents.
const LAUNCHER_ROW_HEIGHT: f64 = 40.0;
/// Vertical padding added around the launcher result list.
const LAUNCHER_PADDING: f64 = 12.0;

/// Horizontal padding inside each launcher result button.
pub(super) const LAUNCHER_ITEM_H_PAD: f64 = 14.0;
/// Width of a launcher result label. Sized so each row spans the dropdown width
/// once the button adds [`LAUNCHER_ITEM_H_PAD`] of padding on each side.
pub(super) const LAUNCHER_ITEM_WIDTH: f64 = LAUNCHER_WIDTH as f64 - 2.0 * LAUNCHER_ITEM_H_PAD;

/// Maximum width of the search field.
pub(super) const SEARCH_BOX_WIDTH: f64 = 420.0;
/// Fixed height of the search field.
pub(super) const SEARCH_BOX_HEIGHT: f64 = 34.0;
/// Horizontal inset between the bar's contents and the screen edges.
pub(super) const SHELL_BAR_H_PAD: f64 = 16.0;

pub(super) const FALLBACK_OUTPUT_KEY: &str = "";

/// Inner size of the launcher dropdown for a given number of results. The
/// dropdown is sized to its contents so it reads as a snug menu.
pub(super) fn launcher_size(result_count: usize) -> LogicalSize<f64> {
    LogicalSize::new(
        f64::from(LAUNCHER_WIDTH),
        result_count as f64 * LAUNCHER_ROW_HEIGHT + LAUNCHER_PADDING,
    )
}
