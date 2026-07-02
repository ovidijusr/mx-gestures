//! Minimal HID++ 2.0 client over a Logitech Unifying receiver.
//!
//! Report formats (first byte = HID report ID):
//!   0x10 short: [id, device_idx, sub_id/feat_idx, fn<<4|sw_id, p0, p1, p2]           (7 bytes)
//!   0x11 long:  [id, device_idx, feat_idx, fn<<4|sw_id, p0..p15]                     (20 bytes)
//! Responses echo (device_idx, feat_idx, fn|sw_id). Events arrive with sw_id = 0.

use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};

pub const VID_LOGITECH: u16 = 0x046D;
/// Unifying receiver. (Bolt is 0xC548 — kept for future-proofing.)
pub const RECEIVER_PIDS: &[u16] = &[0xC52B, 0xC548];
pub const HIDPP_USAGE_PAGE: u16 = 0xFF00;

const REPORT_SHORT: u8 = 0x10;
const REPORT_LONG: u8 = 0x11;
const SW_ID: u8 = 0x0A;

// Feature IDs
pub const FEAT_ROOT: u16 = 0x0000;
pub const FEAT_DEVICE_NAME: u16 = 0x0005;
pub const FEAT_REPROG_CONTROLS_V4: u16 = 0x1B04;
pub const FEAT_WIRELESS_STATUS: u16 = 0x1D4B;

/// Gesture ("palm") button on the MX Master family.
pub const CID_GESTURE: u16 = 0x00C3;

// setCidReporting flag bits
pub const FLAG_DIVERT: u8 = 0x01;
pub const FLAG_DIVERT_VALID: u8 = 0x02;
pub const FLAG_RAW_XY: u8 = 0x10;
pub const FLAG_RAW_XY_VALID: u8 = 0x20;

#[derive(Debug)]
pub enum Error {
    Hid(hidapi::HidError),
    Timeout,
    /// HID++ 1.0 error from the receiver (e.g. no device at index). Code in payload.
    Receiver(u8),
    /// HID++ 2.0 error from the device. Code in payload.
    Device(u8),
    NotFound(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Hid(e) => write!(f, "hid: {e}"),
            Error::Timeout => write!(f, "timeout waiting for HID++ response"),
            Error::Receiver(c) => write!(f, "receiver error 0x{c:02x}"),
            Error::Device(c) => write!(f, "device error 0x{c:02x}"),
            Error::NotFound(what) => write!(f, "not found: {what}"),
        }
    }
}

impl From<hidapi::HidError> for Error {
    fn from(e: hidapi::HidError) -> Self {
        Error::Hid(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// An event notification pushed by the device (sw_id == 0).
#[derive(Debug, Clone)]
pub struct Event {
    pub device_idx: u8,
    /// Feature index (HID++ 2.0) or sub-id (HID++ 1.0 receiver notification).
    pub feat_idx: u8,
    /// High nibble of byte 3 — the event/function number.
    pub event_id: u8,
    pub params: [u8; 16],
}

pub struct Receiver {
    dev: HidDevice,
    pending_events: Vec<Event>,
    pub verbose: bool,
}

impl Receiver {
    /// Open the vendor (0xFF00 usage page) interface of the first Unifying/Bolt receiver.
    pub fn open(api: &HidApi, verbose: bool) -> Result<Self> {
        for info in api.device_list() {
            if info.vendor_id() == VID_LOGITECH
                && RECEIVER_PIDS.contains(&info.product_id())
                && info.usage_page() == HIDPP_USAGE_PAGE
            {
                let dev = info.open_device(api)?;
                return Ok(Receiver { dev, pending_events: Vec::new(), verbose });
            }
        }
        Err(Error::NotFound("Logitech Unifying receiver (vendor HID++ interface)"))
    }

    fn read_raw(&mut self, timeout_ms: i32) -> Result<Option<[u8; 20]>> {
        let mut buf = [0u8; 32];
        let n = self.dev.read_timeout(&mut buf, timeout_ms)?;
        if n == 0 {
            return Ok(None);
        }
        if self.verbose {
            eprintln!("  << {}", hex(&buf[..n]));
        }
        let mut out = [0u8; 20];
        out[..n.min(20)].copy_from_slice(&buf[..n.min(20)]);
        Ok(Some(out))
    }

    /// Send a HID++ 2.0 long request and wait for the matching response.
    /// Events that arrive in the meantime are queued for the main loop.
    pub fn request(
        &mut self,
        device_idx: u8,
        feat_idx: u8,
        function: u8,
        params: &[u8],
    ) -> Result<[u8; 16]> {
        let mut report = [0u8; 20];
        report[0] = REPORT_LONG;
        report[1] = device_idx;
        report[2] = feat_idx;
        report[3] = (function << 4) | SW_ID;
        report[4..4 + params.len()].copy_from_slice(params);
        if self.verbose {
            eprintln!("  >> {}", hex(&report));
        }
        self.dev.write(&report)?;

        let deadline = Instant::now() + Duration::from_millis(2000);
        while Instant::now() < deadline {
            let Some(buf) = self.read_raw(100)? else { continue };
            if buf[1] != device_idx {
                self.queue_if_event(&buf);
                continue;
            }
            // HID++ 1.0 error (receiver-level, e.g. unknown device index)
            if buf[0] == REPORT_SHORT && buf[2] == 0x8F {
                return Err(Error::Receiver(buf[5]));
            }
            // HID++ 2.0 error: [.., 0xFF, feat_idx, fn|sw, code]
            if buf[2] == 0xFF && buf[3] == feat_idx && buf[4] == (function << 4) | SW_ID {
                return Err(Error::Device(buf[5]));
            }
            // Matching response
            if buf[2] == feat_idx && buf[3] == (function << 4) | SW_ID {
                let mut params = [0u8; 16];
                params.copy_from_slice(&buf[4..20]);
                return Ok(params);
            }
            self.queue_if_event(&buf);
        }
        Err(Error::Timeout)
    }

    fn queue_if_event(&mut self, buf: &[u8; 20]) {
        // Events (device notifications) carry sw_id == 0 in the low nibble of byte 3.
        // Receiver 1.0 notifications (e.g. 0x41 device connect) also land here.
        let ev = Event {
            device_idx: buf[1],
            feat_idx: buf[2],
            event_id: buf[3] >> 4,
            params: {
                let mut p = [0u8; 16];
                p.copy_from_slice(&buf[4..20]);
                p
            },
        };
        if buf[3] & 0x0F == 0 || buf[2] == 0x41 {
            self.pending_events.push(ev);
        } else if self.verbose {
            eprintln!("  (dropped unmatched report)");
        }
    }

    /// Next event: queued ones first, then block on the wire up to `timeout_ms`.
    pub fn next_event(&mut self, timeout_ms: i32) -> Result<Option<Event>> {
        if !self.pending_events.is_empty() {
            return Ok(Some(self.pending_events.remove(0)));
        }
        let Some(buf) = self.read_raw(timeout_ms)? else { return Ok(None) };
        self.queue_if_event(&buf);
        if self.pending_events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(self.pending_events.remove(0)))
        }
    }

    // ---- HID++ 2.0 helpers ----

    /// IRoot.getFeature — returns the feature index for a feature ID (0 = not supported).
    pub fn get_feature_index(&mut self, device_idx: u8, feature_id: u16) -> Result<u8> {
        let p = self.request(device_idx, 0x00, 0x0, &feature_id.to_be_bytes())?;
        Ok(p[0])
    }

    pub fn device_name(&mut self, device_idx: u8) -> Result<String> {
        let fi = self.get_feature_index(device_idx, FEAT_DEVICE_NAME)?;
        if fi == 0 {
            return Ok(String::from("(unnamed)"));
        }
        let len = self.request(device_idx, fi, 0x0, &[])?[0] as usize;
        let mut name = Vec::with_capacity(len);
        while name.len() < len {
            let chunk = self.request(device_idx, fi, 0x1, &[name.len() as u8])?;
            let take = (len - name.len()).min(16);
            name.extend_from_slice(&chunk[..take]);
        }
        Ok(String::from_utf8_lossy(&name).trim_end_matches('\0').to_string())
    }

    /// ReprogControlsV4.setCidReporting
    pub fn set_cid_reporting(
        &mut self,
        device_idx: u8,
        reprog_idx: u8,
        cid: u16,
        flags: u8,
    ) -> Result<()> {
        let cid_b = cid.to_be_bytes();
        self.request(device_idx, reprog_idx, 0x3, &[cid_b[0], cid_b[1], flags, 0, 0])?;
        Ok(())
    }

    /// Dump the device's reprogrammable control list (verbose/diagnostic).
    pub fn dump_controls(&mut self, device_idx: u8, reprog_idx: u8) -> Result<Vec<u16>> {
        let count = self.request(device_idx, reprog_idx, 0x0, &[])?[0];
        let mut cids = Vec::new();
        for i in 0..count {
            let p = self.request(device_idx, reprog_idx, 0x1, &[i])?;
            let cid = u16::from_be_bytes([p[0], p[1]]);
            cids.push(cid);
            if self.verbose {
                eprintln!(
                    "  control[{i}]: cid=0x{cid:04X} task=0x{:04X} flags=0x{:02X} extra=0x{:02X}",
                    u16::from_be_bytes([p[2], p[3]]),
                    p[4],
                    p[8],
                );
            }
        }
        Ok(cids)
    }
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}
