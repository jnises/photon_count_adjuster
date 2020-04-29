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

#[derive(Default, NwgUi)]
pub struct Brightness {
    #[nwg_control(size: (300, 115), position: (300, 300), title: "Brightness", flags: "WINDOW|VISIBLE")]
    #[nwg_events( OnWindowClose: [Brightness::quit], OnInit: [Brightness::init] )]
    window: nwg::Window,

    #[nwg_layout(parent: window)]
    layout: nwg::GridLayout,

    #[nwg_control()]
    #[nwg_layout_item(layout: layout, col: 0, row: 0)]
    #[nwg_events( OnComboxBoxSelection: [Brightness::monitor_selected] )]
    monitors: nwg::ComboBox<String>,

    #[nwg_control(range: Some(0..100), pos: Some(50))]
    #[nwg_layout_item(layout: layout, col: 0, row: 1)]
    #[nwg_events( OnMouseMove: [Brightness::brightness_updated] )]
    brightness: nwg::TrackBar,

    monitor_data: RefCell<Vec<ddc_winapi::Monitor>>,
}

impl Brightness {
    fn init(&self) {
        match ddc_winapi::Monitor::enumerate() {
            Ok(monitors) => {
                if !monitors.is_empty() {
                    self.monitors.set_collection(monitors.iter().map(|m| m.description()).collect());
                    *self.monitor_data.borrow_mut() = monitors;
                    self.monitors.set_selection(Some(0));
                    self.monitor_selected();
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

    fn monitor_selected(&self) {
        let handle = self.get_selected_monitor();
        let mut minimum: DWORD = 0;
        let mut maximum: DWORD = 0;
        let mut current: DWORD = 0;
        match unsafe { GetMonitorBrightness(handle, &mut minimum, &mut current, &mut maximum) } {
            0 => {
                self.brightness.set_enabled(false);
                nwg::simple_message("Error", "Unable to get monitor brightness");
            }
            _ => {
                self.brightness.set_selection_range_pos(minimum as usize .. maximum as usize + 1);
                self.brightness.set_pos(current as usize);
                self.brightness.set_enabled(true);
            }
        }
    }

    fn brightness_updated(&self) {
        let handle = self.get_selected_monitor();
        let brightness = self.brightness.pos() as DWORD;
        if unsafe { SetMonitorBrightness(handle, brightness) } == 0 {
                nwg::simple_message("Error", "Unable to set monitor brightness");
        }
    }
}

fn main() {
    nwg::init().expect("Failed to init Native Windows GUI");

    let _app = Brightness::build_ui(Default::default()).expect("Failed to build UI");

    nwg::dispatch_thread_events();
}