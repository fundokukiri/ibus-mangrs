//! Kernel device backend, subject to the mutter-relay ban at the emission layer.

use super::{BackendOps, Kind};
use crate::uinput::{Uinput, KEYCODE_DELETE};

/// The `/dev/uinput` virtual keyboard behind [`BackendOps`]. A unit struct:
/// the device is the process-wide static in `crate::uinput`, so there is no
/// state to carry here.
pub struct UinputBackend;

impl BackendOps for UinputBackend {
    fn kind(&self) -> Kind {
        Kind::Uinput
    }

    fn available(&self) -> bool {
        Uinput::available()
    }

    fn backspaces(&self, n: u32, gap_ms: u64) -> bool {
        Uinput::inject_backspaces(n, gap_ms)
    }

    fn delete(&self, n: u32) -> bool {
        // Whitelist in action: Delete is the only keycode this op can send.
        for _ in 0..n {
            if !Uinput::press_keys(&[KEYCODE_DELETE], 0) {
                return false;
            }
        }
        true
    }

    fn paste(&self) -> bool {
        Uinput::ctrl_v()
    }
}
