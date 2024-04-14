#![warn(clippy::all, rust_2018_idioms)]
#![windows_subsystem = "windows"]

use native_windows_derive as nwd;
use native_windows_gui as nwg;
use nwd::NwgUi;
use nwg::NativeUi;
use std::cell::RefCell;
use winapi::{
    shared::minwindef::DWORD,
    um::{
        highlevelmonitorconfigurationapi::{GetMonitorBrightness, SetMonitorBrightness},
        winnt::HANDLE,
    },
};

fn get_exe_icon() -> Option<nwg::Icon> {
    nwg::EmbedResource::load(None).ok()?.icon(1, None)
}

#[derive(Default, NwgUi)]
pub struct PhotonCountAdjuster {
    #[nwg_resource(family: "Sergoe UI", size: 16)]
    font: nwg::Font,

    #[nwg_control(size: (300, 115), position: (300, 300), title: "Photon count adjuster", flags: "WINDOW|VISIBLE", icon: get_exe_icon().as_ref())]
    #[nwg_events( OnWindowClose: [PhotonCountAdjuster::quit], OnInit: [PhotonCountAdjuster::init] )]
    window: nwg::Window,

    #[nwg_layout(parent: window)]
    layout: nwg::GridLayout,

    #[nwg_control(font: Some(&data.font))]
    #[nwg_layout_item(layout: layout, col: 0, row: 0)]
    #[nwg_events( OnComboxBoxSelection: [PhotonCountAdjuster::monitor_selected] )]
    monitors: nwg::ComboBox<String>,

    #[nwg_control(range: Some(0..100), pos: Some(50))]
    #[nwg_layout_item(layout: layout, col: 0, row: 1)]
    #[nwg_events( OnMouseMove: [PhotonCountAdjuster::brightness_slider_updated] )]
    brightness_slider: nwg::TrackBar,

    #[nwg_control()]
    #[nwg_layout_item(layout: layout, col: 0, row: 2)]
    status_message: nwg::Label,

    monitor_data: RefCell<Vec<ddc_winapi::Monitor>>,
}

impl PhotonCountAdjuster {
    fn init(&self) {
        self.set_status("");
        match ddc_winapi::Monitor::enumerate() {
            Ok(monitors) => {
                if !monitors.is_empty() {
                    self.monitors
                        .set_collection(monitors.iter().map(|m| m.description()).collect());
                    *self.monitor_data.borrow_mut() = monitors;
                    self.brightness_slider.set_enabled(false);
                    for (idx, m) in self.monitor_data.borrow().iter().enumerate() {
                        if self.update_monitor_brightness(m.handle()) {
                            self.monitors.set_selection(Some(idx));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                nwg::error_message("Error enumerating monitors", &format!("{}", e));
            }
        }
    }

    fn quit(&self) {
        nwg::stop_thread_dispatch();
    }

    fn get_selected_monitor(&self) -> HANDLE {
        let idx = match self.monitors.selection() {
            Some(idx) => idx,
            None => {
                nwg::fatal_message("Error", "expected monitor selection");
            }
        };
        match self.monitor_data.borrow().get(idx) {
            Some(m) => m.handle(),
            None => {
                nwg::fatal_message("Error", "monitor index out of bounds");
            }
        }
    }

    /// returns true if successful
    fn update_monitor_brightness(&self, handle: HANDLE) -> bool {
        let mut minimum: DWORD = 0;
        let mut maximum: DWORD = 0;
        let mut current: DWORD = 0;
        match unsafe { GetMonitorBrightness(handle, &mut minimum, &mut current, &mut maximum) } {
            0 => {
                self.brightness_slider.set_enabled(false);
                let mut icon = nwg::Icon::default();
                if let Err(e) = nwg::Icon::builder()
                    .source_system(Some(nwg::OemIcon::Warning))
                    .build(&mut icon)
                {
                    nwg::fatal_message("Error", &format!("Unable to load icon: {e}"));
                }
                self.set_status("Unable to control brightness for this monitor");
                false
            }
            _ => {
                self.brightness_slider
                    .set_selection_range_pos(minimum as usize..maximum as usize + 1);
                self.brightness_slider.set_pos(current as usize);
                self.brightness_slider.set_enabled(true);
                self.set_status("");
                true
            }
        }
    }

    fn monitor_selected(&self) {
        let handle = self.get_selected_monitor();
        self.update_monitor_brightness(handle);
    }

    fn brightness_slider_updated(&self) {
        let handle = self.get_selected_monitor();
        let brightness = self.brightness_slider.pos() as DWORD;
        if unsafe { SetMonitorBrightness(handle, brightness) } == 0 {
            nwg::simple_message("Error", "Unable to set monitor brightness");
        }
    }

    fn set_status(&self, message: &str) {
        self.status_message.set_text(message);
        self.status_message.set_visible(!message.is_empty());
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    let _app = PhotonCountAdjuster::build_ui(Default::default()).expect("Failed to build UI");
    nwg::dispatch_thread_events();
}
