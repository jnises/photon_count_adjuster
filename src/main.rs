#![windows_subsystem = "windows"]

extern crate native_windows_gui as nwg;
extern crate native_windows_derive as nwd;
use nwd::NwgUi;
use nwg::NativeUi;
use std::cell::RefCell;
use winapi::{
    um::{
        highlevelmonitorconfigurationapi::{
            GetMonitorBrightness,
            SetMonitorBrightness,
        },
        winnt::HANDLE,
    },
    shared::minwindef::DWORD,
};

fn get_exe_icon() -> Option<nwg::Icon> {
    Some(nwg::EmbedResource::load(None).ok()?.icon(1, None)?)
}

#[derive(Default, NwgUi)]
pub struct PhotonCountAdjuster {
    #[nwg_resource(family: "Arial", size: 16)]
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

    monitor_data: RefCell<Vec<ddc_winapi::Monitor>>,
}

impl PhotonCountAdjuster {
    fn init(&self) {
        match ddc_winapi::Monitor::enumerate() {
            Ok(monitors) => {
                if !monitors.is_empty() {
                    self.monitors.set_collection(monitors.iter().map(|m| m.description()).collect());
                    *self.monitor_data.borrow_mut() = monitors;
                    self.brightness_slider.set_enabled(false);
                    for (idx, m) in self.monitor_data.borrow().iter().enumerate() {
                        if let Ok(_) = self.try_get_monitor_brightness(m.handle()) {
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

    fn try_get_monitor_brightness(&self, handle: HANDLE) -> Result<(), ()> {
        let mut minimum: DWORD = 0;
        let mut maximum: DWORD = 0;
        let mut current: DWORD = 0;
        match unsafe { GetMonitorBrightness(handle, &mut minimum, &mut current, &mut maximum) } {
            0 => {
                self.brightness_slider.set_enabled(false);
                Err(())
            }
            _ => {
                self.brightness_slider.set_selection_range_pos(minimum as usize .. maximum as usize + 1);
                self.brightness_slider.set_pos(current as usize);
                self.brightness_slider.set_enabled(true);
                Ok(())
            }
        }

    }

    fn monitor_selected(&self) {
        let handle = self.get_selected_monitor();
        if let Err(_) = self.try_get_monitor_brightness(handle) {
            nwg::simple_message("Error", "Unable to get monitor brightness");
        }
    }

    fn brightness_slider_updated(&self) {
        let handle = self.get_selected_monitor();
        let brightness = self.brightness_slider.pos() as DWORD;
        if unsafe { SetMonitorBrightness(handle, brightness) } == 0 {
                nwg::simple_message("Error", "Unable to set monitor brightness");
        }
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");
    let _app = PhotonCountAdjuster::build_ui(Default::default()).expect("Failed to build UI");
    nwg::dispatch_thread_events();
}