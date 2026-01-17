use console_draw::{TextScreen, V2DisplayMap};
use drm::buffer::Buffer;
use drm::control::Device;
use graphics_ipc::v2::{Damage, V2GraphicsHandle};
use inputd::ConsumerHandle;
use std::io;

pub struct Display {
    pub input_handle: ConsumerHandle,
    pub map: Option<V2DisplayMap>,
}

impl Display {
    fn debug_num(n: u8) {
        // Write debug number to kernel debug console (blocking-safe)
        let _ = std::fs::write("/scheme/debug/no-preserve", &[b'F', n + b'0', b'\n']);
    }

    pub fn open_new_vt() -> io::Result<Self> {
        Self::debug_num(1); // F1 = entered open_new_vt
        eprintln!("fbcond: open_new_vt: about to call ConsumerHandle::new_vt()");
        let input_handle = ConsumerHandle::new_vt()?;
        Self::debug_num(2); // F2 = ConsumerHandle created
        eprintln!("fbcond: open_new_vt: ConsumerHandle created");

        let mut display = Self {
            input_handle,
            map: None,
        };

        Self::debug_num(3); // F3 = calling reopen_for_handoff
        display.reopen_for_handoff();
        Self::debug_num(4); // F4 = reopen_for_handoff returned

        Ok(display)
    }

    /// Re-open the display after a handoff.
    pub fn reopen_for_handoff(&mut self) {
        Self::debug_num(5); // F5 = entered reopen_for_handoff
        // Skip eprintln which blocks on logd
        let display_file = match self.input_handle.open_display_v2() {
            Ok(f) => {
                Self::debug_num(6); // F6 = open_display_v2 OK
                f
            }
            Err(err) => {
                // Error - write Fx to debug
                let _ = std::fs::write("/scheme/debug/no-preserve", b"FE\n");
                return;
            }
        };
        Self::debug_num(7); // F7 = creating graphics handle
        let new_display_handle = match V2GraphicsHandle::from_file(display_file) {
            Ok(h) => h,
            Err(err) => {
                let _ = std::fs::write("/scheme/debug/no-preserve", b"F7E\n");
                return;
            }
        };

        Self::debug_num(8); // F8 = getting first display
        let first_display = match new_display_handle.first_display() {
            Ok(d) => d,
            Err(err) => {
                let _ = std::fs::write("/scheme/debug/no-preserve", b"F8E\n");
                return;
            }
        };

        Self::debug_num(9); // F9 = getting connector
        let connector = match new_display_handle.get_connector(first_display, true) {
            Ok(c) => c,
            Err(err) => {
                let _ = std::fs::write("/scheme/debug/no-preserve", b"F9E\n");
                return;
            }
        };
        let modes = connector.modes();
        if modes.is_empty() {
            let _ = std::fs::write("/scheme/debug/no-preserve", b"F9N\n");
            return;
        }
        let (width, height) = modes[0].size();

        let _ = std::fs::write("/scheme/debug/no-preserve", b"FA\n"); // FA = creating display map
        match V2DisplayMap::new(new_display_handle, width.into(), height.into()) {
            Ok(map) => {
                self.map = Some(map);
                let _ = std::fs::write("/scheme/debug/no-preserve", b"FB\n"); // FB = display map OK
            }
            Err(err) => {
                let _ = std::fs::write("/scheme/debug/no-preserve", b"FAE\n");
                return;
            }
        }
    }

    pub fn handle_resize(map: &mut V2DisplayMap, text_screen: &mut TextScreen) {
        let (width, height) = match map.display_handle.first_display().and_then(|handle| {
            Ok(map.display_handle.get_connector(handle, true)?.modes()[0].size())
        }) {
            Ok((width, height)) => (width.into(), height.into()),
            Err(err) => {
                log::error!("fbcond: failed to get display size: {}", err);
                map.fb.size()
            }
        };

        if (width, height) != map.fb.size() {
            match text_screen.resize(map, width, height) {
                Ok(()) => eprintln!("fbcond: mapped display"),
                Err(err) => {
                    eprintln!("fbcond: failed to create or map framebuffer: {}", err);
                    return;
                }
            }
        }
    }

    pub fn sync_rect(&mut self, damage: Damage) {
        if let Some(map) = &self.map {
            map.display_handle
                .update_plane(0, u32::from(map.fb.handle()), damage)
                .unwrap();
        }
    }
}
