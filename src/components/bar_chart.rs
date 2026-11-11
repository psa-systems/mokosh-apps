//! MAPPS-297: SVG bar chart for the report-detail surface.
//!
//! Hand-rolled SVG so the SPA does not pull a JS charting library into
//! the wasm bundle just to render N labelled bars. The chart auto-
//! scales to the maximum value, falls back to a no-data state when
//! the series is empty, and exposes a `unit_suffix` so a caller can
//! render percentages, hours, currency, etc.

use dioxus::prelude::*;

/// One row in the bar chart. Label sits on the left rail; the bar
/// renders to the right scaled against `BarChartProps::values`'s max.
#[derive(Clone, Debug, PartialEq)]
pub struct BarChartDatum {
    pub label: String,
    pub value: f64,
}

#[derive(Props, Clone, PartialEq)]
pub struct BarChartProps {
    pub data: Vec<BarChartDatum>,
    /// Appended to the per-row numeric label (" hours", "%", " tickets").
    #[props(default)]
    pub unit_suffix: String,
    /// Render value labels at 1 decimal place. Off by default (integer
    /// values display as-is).
    #[props(default = false)]
    pub one_decimal: bool,
    /// The chart's accessible name (MAPPS-1243). The SVG is `role="img"`, so
    /// without one a screen reader announced an unnamed image.
    #[props(default = "Bar chart".to_string())]
    pub title: String,
}

/// The value text one row shows, also read out by the text alternative.
fn value_label(value: f64, one_decimal: bool, unit_suffix: &str) -> String {
    if one_decimal {
        format!("{value:.1}{unit_suffix}")
    } else if value.fract() == 0.0 {
        format!("{}{unit_suffix}", value as i64)
    } else {
        format!("{value}{unit_suffix}")
    }
}

#[component]
pub fn BarChart(props: BarChartProps) -> Element {
    if props.data.is_empty() {
        return rsx! {
            p { class: "text-sm text-subtle italic", "No data to chart." }
        };
    }
    let max = props
        .data
        .iter()
        .map(|d| d.value)
        .fold(0.0_f64, f64::max)
        .max(1.0);

    let bar_height = 18;
    let row_gap = 10;
    let label_col = 160;
    let value_col = 80;
    let bar_track_width = 320;
    let row_height = bar_height + row_gap;
    let total_height = props.data.len() as i32 * row_height + 10;
    let total_width = label_col + bar_track_width + value_col + 20;

    rsx! {
        div { class: "overflow-x-auto",
            // MAPPS-1243: the data as text, for a screen reader. The drawn
            // labels are SVG `text` inside an image, which assistive tech
            // does not read row by row.
            ul { class: "sr-only",
                for d in props.data.iter() {
                    li { "{d.label}: {value_label(d.value, props.one_decimal, &props.unit_suffix)}" }
                }
            }
            svg {
                role: "img",
                "aria-label": "{props.title}",
                width: "{total_width}",
                height: "{total_height}",
                view_box: "0 0 {total_width} {total_height}",
                for (i, d) in props.data.iter().enumerate() {
                    {
                        let value = d.value;
                        let pct = (value / max).clamp(0.0, 1.0);
                        let bar_width = (pct * bar_track_width as f64) as i32;
                        let y = i as i32 * row_height + 5;
                        let label_y = y + bar_height - 4;
                        let bar_x = label_col;
                        let value_x = label_col + bar_track_width + 8;
                        let value_label = value_label(value, props.one_decimal, &props.unit_suffix);
                        rsx! {
                            text {
                                x: "0",
                                y: "{label_y}",
                                class: "fill-current text-content text-xs",
                                "{d.label}"
                            }
                            rect {
                                x: "{bar_x}",
                                y: "{y}",
                                width: "{bar_track_width}",
                                height: "{bar_height}",
                                class: "fill-surface-2",
                            }
                            rect {
                                x: "{bar_x}",
                                y: "{y}",
                                width: "{bar_width}",
                                height: "{bar_height}",
                                class: "fill-accent",
                            }
                            text {
                                x: "{value_x}",
                                y: "{label_y}",
                                class: "fill-current text-content text-xs font-medium",
                                "{value_label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// MAPPS-1243: the chart has an accessible name and its data as text.
#[cfg(test)]
mod tests {
    use super::value_label;

    #[test]
    fn value_labels_read_as_before() {
        assert_eq!(value_label(3.0, false, " hours"), "3 hours");
        assert_eq!(value_label(2.5, false, "%"), "2.5%");
        assert_eq!(value_label(2.0, true, ""), "2.0");
    }

    #[test]
    fn the_svg_is_named_and_the_rows_are_text() {
        let src = include_str!("bar_chart.rs");
        let code = &src[..src.find("mod tests {").unwrap()];
        let flat = code.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("role: \"img\", \"aria-label\": \"{props.title}\","));
        assert!(flat.contains("ul { class: \"sr-only\","));
    }
}
