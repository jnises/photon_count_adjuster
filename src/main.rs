#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(any(target_os = "macos", windows)))]
compile_error!("photon_count_adjuster supports macOS and Windows");

use ddc_hi::{Ddc, Display};
use eframe::egui;
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

const BRIGHTNESS_VCP_CODE: u8 = 0x10;
const BRIGHTNESS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const DISPLAY_SCAN_INTERVAL: Duration = Duration::from_secs(10);
const WORKER_TICK: Duration = Duration::from_millis(250);

#[derive(Clone)]
struct MonitorSnapshot {
    id: String,
    label: String,
    brightness: u16,
    maximum: u16,
    error: Option<String>,
}

struct MonitorUpdate {
    monitors: Vec<MonitorSnapshot>,
    error: Option<String>,
}

struct MonitorDevice {
    display: Display,
    snapshot: MonitorSnapshot,
}

impl MonitorDevice {
    fn new(display: Display) -> Self {
        let snapshot = MonitorSnapshot {
            id: display.info.id.clone(),
            label: display.info.to_string(),
            brightness: 0,
            maximum: 100,
            error: None,
        };
        let mut monitor = Self { display, snapshot };
        monitor.refresh();
        monitor
    }

    fn refresh(&mut self) {
        match self.display.handle.get_vcp_feature(BRIGHTNESS_VCP_CODE) {
            Ok(value) if value.maximum() > 0 => {
                self.snapshot.brightness = value.value();
                self.snapshot.maximum = value.maximum();
                self.snapshot.error = None;
            }
            Ok(_) => {
                self.snapshot.error =
                    Some("Monitor reported an invalid brightness range".to_owned());
            }
            Err(error) => {
                self.snapshot.error = Some(format!("Unable to read brightness: {error}"));
            }
        }
    }

    fn set_brightness(&mut self, brightness: u16) {
        match self
            .display
            .handle
            .set_vcp_feature(BRIGHTNESS_VCP_CODE, brightness)
        {
            Ok(()) => {
                self.snapshot.brightness = brightness;
                self.snapshot.error = None;
            }
            Err(error) => {
                self.snapshot.error = Some(format!("Unable to set brightness: {error}"));
            }
        }
    }
}

enum MonitorCommand {
    SetPolling(bool),
    Rescan,
}

fn scan_displays() -> Vec<MonitorDevice> {
    Display::enumerate()
        .into_iter()
        .map(MonitorDevice::new)
        .collect()
}

fn publish_snapshots(
    monitors: &[MonitorDevice],
    error: Option<&str>,
    updates: &Sender<MonitorUpdate>,
    ctx: &egui::Context,
) -> bool {
    let update = MonitorUpdate {
        monitors: monitors
            .iter()
            .map(|monitor| monitor.snapshot.clone())
            .collect(),
        error: error.map(str::to_owned),
    };
    if updates.send(update).is_err() {
        // The UI owns the receiver, so disconnection means the application has closed.
        return false;
    }
    ctx.request_repaint();
    true
}

fn select_monitor(monitors: &[MonitorSnapshot], selected_id: Option<&str>) -> usize {
    selected_id
        .and_then(|id| monitors.iter().position(|monitor| monitor.id == id))
        .or_else(|| monitors.iter().position(|monitor| monitor.error.is_none()))
        .unwrap_or_default()
}

fn run_monitor_worker(
    commands: Receiver<MonitorCommand>,
    updates: Sender<MonitorUpdate>,
    brightness_request: Arc<Mutex<Option<(String, u16)>>>,
    ctx: egui::Context,
) {
    let mut monitors = scan_displays();
    if !publish_snapshots(&monitors, None, &updates, &ctx) {
        return;
    }

    let mut polling = false;
    let mut worker_error = None;
    let mut last_brightness_refresh = Instant::now();
    let mut last_display_scan = Instant::now();

    loop {
        let mut changed = false;
        // A single shared slot coalesces rapid slider events while a slow DDC write is in flight.
        let requested_brightness = brightness_request
            .lock()
            .expect("brightness request mutex poisoned")
            .take();
        if let Some((id, brightness)) = requested_brightness {
            if let Some(monitor) = monitors
                .iter_mut()
                .find(|monitor| monitor.snapshot.id == id)
            {
                monitor.set_brightness(brightness);
                worker_error = None;
            } else {
                worker_error = Some(format!(
                    "Unable to set brightness: display {id} is no longer connected"
                ));
            }
            last_brightness_refresh = Instant::now();
            changed = true;
        }

        match commands.recv_timeout(WORKER_TICK) {
            Ok(MonitorCommand::SetPolling(enabled)) => {
                if enabled && !polling {
                    monitors = scan_displays();
                    worker_error = None;
                    let now = Instant::now();
                    last_brightness_refresh = now;
                    last_display_scan = now;
                    changed = true;
                }
                polling = enabled;
            }
            Ok(MonitorCommand::Rescan) => {
                monitors = scan_displays();
                worker_error = None;
                let now = Instant::now();
                last_brightness_refresh = now;
                last_display_scan = now;
                changed = true;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // The command sender is owned by the UI, so disconnection means it has closed.
                break;
            }
        }

        let now = Instant::now();
        if polling && now.duration_since(last_display_scan) >= DISPLAY_SCAN_INTERVAL {
            monitors = scan_displays();
            worker_error = None;
            last_brightness_refresh = now;
            last_display_scan = now;
            changed = true;
        } else if polling
            && now.duration_since(last_brightness_refresh) >= BRIGHTNESS_REFRESH_INTERVAL
        {
            for monitor in &mut monitors {
                monitor.refresh();
            }
            worker_error = None;
            last_brightness_refresh = now;
            changed = true;
        }

        if changed && !publish_snapshots(&monitors, worker_error.as_deref(), &updates, &ctx) {
            break;
        }
    }
}

struct PhotonCountAdjuster {
    monitors: Vec<MonitorSnapshot>,
    selected: usize,
    commands: Sender<MonitorCommand>,
    updates: Receiver<MonitorUpdate>,
    brightness_request: Arc<Mutex<Option<(String, u16)>>>,
    polling_enabled: bool,
    slider_active: bool,
    worker_error: Option<String>,
}

impl PhotonCountAdjuster {
    fn new(ctx: &egui::Context) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::channel();
        let brightness_request = Arc::new(Mutex::new(None));
        let worker_brightness_request = Arc::clone(&brightness_request);
        let worker_ctx = ctx.clone();
        std::thread::Builder::new()
            .name("monitor-ddc".to_owned())
            .spawn(move || {
                run_monitor_worker(command_rx, update_tx, worker_brightness_request, worker_ctx)
            })
            .expect("failed to start DDC monitor worker");
        Self {
            monitors: Vec::new(),
            selected: 0,
            commands: command_tx,
            updates: update_rx,
            brightness_request,
            polling_enabled: false,
            slider_active: false,
            worker_error: None,
        }
    }

    fn send_command(&mut self, command: MonitorCommand) {
        if let Err(error) = self.commands.send(command) {
            self.worker_error = Some(format!("Monitor worker stopped: {error}"));
        }
    }

    fn apply_updates(&mut self) {
        for update in self.updates.try_iter() {
            let selected_id = self
                .monitors
                .get(self.selected)
                .map(|monitor| monitor.id.as_str());
            let selected = select_monitor(&update.monitors, selected_id);
            self.monitors = update.monitors;
            self.selected = selected;
            self.worker_error = update.error;
        }
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
        self.apply_updates();
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
        let polling_enabled = focused && !self.slider_active;
        if polling_enabled != self.polling_enabled {
            self.send_command(MonitorCommand::SetPolling(polling_enabled));
            self.polling_enabled = polling_enabled;
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.set_min_size(ui.available_size());

            if self.monitors.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("No DDC/CI displays found.");
                    if let Some(error) = &self.worker_error {
                        ui.colored_label(ui.visuals().error_fg_color, "Error")
                            .on_hover_text(error);
                    }
                });
                ui.add_space(6.0);
                if rescan_button(ui).clicked() {
                    self.send_command(MonitorCommand::Rescan);
                }
                self.slider_active = false;
                return;
            }

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Display").small().weak());
                if let Some(error) = &self.worker_error {
                    ui.colored_label(ui.visuals().error_fg_color, "Error")
                        .on_hover_text(error);
                }
            });
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
                self.send_command(MonitorCommand::Rescan);
            }

            let brightness_change = {
                let monitor = &mut self.monitors[self.selected];
                ui.add_space(10.0);
                let brightness_label = ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Brightness").small().weak());
                    if let Some(error) = &monitor.error {
                        ui.colored_label(ui.visuals().error_fg_color, "Error")
                            .on_hover_text(error);
                    }
                });
                if let Some(error) = &monitor.error {
                    brightness_label.response.on_hover_text(error);
                }

                ui.spacing_mut().slider_width = (ui.available_width() - 55.0).max(100.0);
                let slider = egui::Slider::new(&mut monitor.brightness, 0..=monitor.maximum);
                let response = ui.add_enabled_ui(monitor.error.is_none(), |ui| ui.add(slider));
                let response = response.inner;
                self.slider_active = response.dragged();
                response
                    .changed()
                    .then(|| (monitor.id.clone(), monitor.brightness))
            };
            if let Some((id, brightness)) = brightness_change {
                *self
                    .brightness_request
                    .lock()
                    .expect("brightness request mutex poisoned") = Some((id, brightness));
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
        Box::new(|creation_context| {
            Ok(Box::new(PhotonCountAdjuster::new(
                &creation_context.egui_ctx,
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{MonitorSnapshot, select_monitor};

    fn monitor(id: &str, controllable: bool) -> MonitorSnapshot {
        MonitorSnapshot {
            id: id.to_owned(),
            label: id.to_owned(),
            brightness: 50,
            maximum: 100,
            error: (!controllable).then(|| "unavailable".to_owned()),
        }
    }

    #[test]
    fn preserves_selected_monitor_after_rescan() {
        let monitors = vec![monitor("first", true), monitor("selected", true)];
        assert_eq!(select_monitor(&monitors, Some("selected")), 1);
    }

    #[test]
    fn falls_back_to_first_controllable_monitor() {
        let monitors = vec![monitor("unavailable", false), monitor("working", true)];
        assert_eq!(select_monitor(&monitors, Some("disconnected")), 1);
    }
}
