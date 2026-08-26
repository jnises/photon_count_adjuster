#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(any(target_os = "macos", windows)))]
compile_error!("photon_count_adjuster supports macOS and Windows");

use ddc_hi::{Ddc, Display};
use eframe::egui;
use std::time::{Duration, Instant};

const BRIGHTNESS_VCP_CODE: u8 = 0x10;
const BRIGHTNESS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const DISPLAY_SCAN_INTERVAL: Duration = Duration::from_secs(10);

struct Monitor {
    display: Display,
    label: String,
    brightness: u16,
    maximum: u16,
    error: Option<String>,
}

impl Monitor {
    fn new(display: Display) -> Self {
        let label = display.info.to_string();
        let mut monitor = Self {
            display,
            label,
            brightness: 0,
            maximum: 100,
            error: None,
        };
        monitor.refresh();
        monitor
    }

    fn refresh(&mut self) {
        match self.display.handle.get_vcp_feature(BRIGHTNESS_VCP_CODE) {
            Ok(value) if value.maximum() > 0 => {
                self.brightness = value.value();
                self.maximum = value.maximum();
                self.error = None;
            }
            Ok(_) => {
                self.error = Some("Monitor reported an invalid brightness range".to_owned());
            }
            Err(error) => {
                self.error = Some(format!("Unable to read brightness: {error}"));
            }
        }
    }

    fn set_brightness(&mut self) {
        match self
            .display
            .handle
            .set_vcp_feature(BRIGHTNESS_VCP_CODE, self.brightness)
        {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(format!("Unable to set brightness: {error}")),
        }
    }
}

struct PhotonCountAdjuster {
    monitors: Vec<Monitor>,
    selected: usize,
    last_brightness_refresh: Instant,
    last_display_scan: Instant,
    window_was_focused: bool,
    slider_active: bool,
}

impl Default for PhotonCountAdjuster {
    fn default() -> Self {
        let now = Instant::now();
        let monitors: Vec<_> = Display::enumerate().into_iter().map(Monitor::new).collect();
        let selected = monitors
            .iter()
            .position(|monitor| monitor.error.is_none())
            .unwrap_or_default();
        Self {
            monitors,
            selected,
            last_brightness_refresh: now,
            last_display_scan: now,
            window_was_focused: false,
            slider_active: false,
        }
    }
}

impl PhotonCountAdjuster {
    fn refresh_selected(&mut self) {
        if let Some(monitor) = self.monitors.get_mut(self.selected) {
            monitor.refresh();
        }
        self.last_brightness_refresh = Instant::now();
    }

    fn scan_displays(&mut self) {
        let selected_id = self
            .monitors
            .get(self.selected)
            .map(|monitor| monitor.display.info.id.clone());
        let monitors: Vec<_> = Display::enumerate().into_iter().map(Monitor::new).collect();
        let selected = selected_id
            .as_deref()
            .and_then(|id| {
                monitors
                    .iter()
                    .position(|monitor| monitor.display.info.id == id)
            })
            .or_else(|| monitors.iter().position(|monitor| monitor.error.is_none()))
            .unwrap_or_default();
        let now = Instant::now();
        self.monitors = monitors;
        self.selected = selected;
        self.last_brightness_refresh = now;
        self.last_display_scan = now;
    }
}

fn rescan_button(ui: &mut egui::Ui) -> egui::Response {
    let size = egui::vec2(ui.spacing().interact_size.y, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Rescan displays")
    });

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let rect = rect.expand(visuals.expansion);
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );

        let radius = rect.width() * 0.22;
        let center = rect.center();
        let start = 0.15 * std::f32::consts::PI;
        let end = 1.75 * std::f32::consts::PI;
        let points = (0..=14)
            .map(|step| {
                let angle = egui::lerp(start..=end, step as f32 / 14.0);
                center + radius * egui::vec2(angle.cos(), angle.sin())
            })
            .collect();
        ui.painter()
            .add(egui::Shape::line(points, visuals.fg_stroke));

        let tip = center + radius * egui::vec2(end.cos(), end.sin());
        let tangent = egui::vec2(-end.sin(), end.cos());
        let inward = (center - tip).normalized();
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                tip,
                tip - tangent * 3.0 + inward * 4.0,
                tip + tangent * 3.0 + inward * 4.0,
            ],
            visuals.fg_stroke.color,
            egui::Stroke::NONE,
        ));
    }

    response.on_hover_text("Rescan displays")
}

impl eframe::App for PhotonCountAdjuster {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
        let focus_gained = focused && !self.window_was_focused;
        let now = Instant::now();

        // DDC/CI has no change notifications, so bounded polling keeps external changes visible.
        if focused && !self.slider_active {
            if focus_gained || now.duration_since(self.last_display_scan) >= DISPLAY_SCAN_INTERVAL {
                self.scan_displays();
            } else if now.duration_since(self.last_brightness_refresh)
                >= BRIGHTNESS_REFRESH_INTERVAL
            {
                self.refresh_selected();
            }
        }

        self.window_was_focused = focused;
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.set_min_size(ui.available_size());

            if self.monitors.is_empty() {
                ui.label("No DDC/CI displays found.");
                ui.add_space(6.0);
                if rescan_button(ui).clicked() {
                    self.scan_displays();
                }
                self.slider_active = false;
                return;
            }

            let previous_selection = self.selected;
            ui.label(egui::RichText::new("Display").small().weak());
            let scan = ui
                .horizontal(|ui| {
                    let button_width = ui.spacing().interact_size.y;
                    let combo_width =
                        ui.available_width() - button_width - ui.spacing().item_spacing.x;
                    egui::ComboBox::from_id_salt("display")
                        .selected_text(&self.monitors[self.selected].label)
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            for (index, monitor) in self.monitors.iter().enumerate() {
                                ui.selectable_value(&mut self.selected, index, &monitor.label);
                            }
                        });
                    rescan_button(ui).clicked()
                })
                .inner;

            if scan {
                self.scan_displays();
                return;
            }

            if self.selected != previous_selection {
                self.refresh_selected();
            }

            {
                let monitor = &mut self.monitors[self.selected];
                ui.add_space(10.0);
                ui.label(egui::RichText::new("Brightness").small().weak());
                ui.spacing_mut().slider_width = (ui.available_width() - 55.0).max(100.0);
                let slider = egui::Slider::new(&mut monitor.brightness, 0..=monitor.maximum);
                let response = ui.add_enabled_ui(monitor.error.is_none(), |ui| ui.add(slider));
                let response = response.inner;
                self.slider_active = response.dragged();
                if response.changed() {
                    monitor.set_brightness();
                    self.last_brightness_refresh = Instant::now();
                }

                if let Some(error) = &monitor.error {
                    ui.add_space(8.0);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([420.0, 110.0])
            .with_min_inner_size([340.0, 105.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Photon count adjuster",
        options,
        Box::new(|_creation_context| Ok(Box::<PhotonCountAdjuster>::default())),
    )
}
