//! `virtio-gpu` is a virtio based graphics adapter. It can operate in 2D mode and in 3D mode.
//!
//! XXX: 3D mode will offload rendering ops to the host gpu and therefore requires a GPU with 3D support
//! on the host machine.

// Notes for the future:
//
// `virtio-gpu` 2D acceleration is just blitting. 3D acceleration has 2 kinds:
//      - virgl - OpenGL
//      - venus - Vulkan
//
// The Venus driver requires support for the following from the `virtio-gpu` kernel driver:
//     - VIRTGPU_PARAM_3D_FEATURES
//     - VIRTGPU_PARAM_CAPSET_QUERY_FIX
//     - VIRTGPU_PARAM_RESOURCE_BLOB
//     - VIRTGPU_PARAM_HOST_VISIBLE
//     - VIRTGPU_PARAM_CROSS_DEVICE
//     - VIRTGPU_PARAM_CONTEXT_INIT
//
// cc https://docs.mesa3d.org/drivers/venus.html
// cc https://docs.mesa3d.org/drivers/virgl.html

use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, Ordering};

use driver_graphics::GraphicsAdapter;
use syscall::EventFlags;
use event::{user_data, EventQueue};
use pcid_interface::PciFunctionHandle;

use virtio_core::utils::VolatileCell;
use virtio_core::MSIX_PRIMARY_VECTOR;

mod scheme;

//const VIRTIO_GPU_F_VIRGL: u32 = 0;
const VIRTIO_GPU_F_EDID: u32 = 1;
//const VIRTIO_GPU_F_RESOURCE_UUID: u32 = 2;
//const VIRTIO_GPU_F_RESOURCE_BLOB: u32 = 3;
//const VIRTIO_GPU_F_CONTEXT_INIT: u32 = 4;

const VIRTIO_GPU_EVENT_DISPLAY: u32 = 1 << 0;
const VIRTIO_GPU_MAX_SCANOUTS: usize = 16;

#[repr(C)]
pub struct GpuConfig {
    /// Signals pending events to the driver.
    pub events_read: VolatileCell<u32>, // read-only
    /// Clears pending events in the device (write-to-clear).
    pub events_clear: VolatileCell<u32>, // write-only

    pub num_scanouts: VolatileCell<u32>,
    pub num_capsets: VolatileCell<u32>,
}

impl GpuConfig {
    #[inline]
    pub fn num_scanouts(&self) -> u32 {
        self.num_scanouts.get()
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(u32)]
pub enum CommandTy {
    Undefined = 0,

    // 2D commands
    GetDisplayInfo = 0x0100,
    ResourceCreate2d,
    ResourceUnref,
    SetScanout,
    ResourceFlush,
    TransferToHost2d,
    ResourceAttachBacking,
    ResourceDetachBacking,
    GetCapsetInfo,
    GetCapset,
    GetEdid,
    ResourceAssignUuid,
    ResourceCreateBlob,
    SetScanoutBlob,

    // 3D commands
    CtxCreate = 0x0200,
    CtxDestroy,
    CtxAttachResource,
    CtxDetachResource,
    ResourceCreate3d,
    TransferToHost3d,
    TransferFromHost3d,
    Submit3d,
    ResourceMapBlob,
    ResourceUnmapBlob,

    // cursor commands
    UpdateCursor = 0x0300,
    MoveCursor,

    // success responses
    RespOkNodata = 0x1100,
    RespOkDisplayInfo,
    RespOkCapsetInfo,
    RespOkCapset,
    RespOkEdid,
    RespOkResourceUuid,
    RespOkMapInfo,

    // error responses
    RespErrUnspec = 0x1200,
    RespErrOutOfMemory,
    RespErrInvalidScanoutId,
    RespErrInvalidResourceId,
    RespErrInvalidContextId,
    RespErrInvalidParameter,
}

static_assertions::const_assert_eq!(core::mem::size_of::<CommandTy>(), 4);

const VIRTIO_GPU_FLAG_FENCE: u32 = 1 << 0;
//const VIRTIO_GPU_FLAG_INFO_RING_IDX: u32 = 1 << 1;

#[derive(Debug)]
#[repr(C)]
pub struct ControlHeader {
    pub ty: CommandTy,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_index: u8,
    padding: [u8; 3],
}

impl ControlHeader {
    pub fn with_ty(ty: CommandTy) -> Self {
        Self {
            ty,
            ..Default::default()
        }
    }
}

impl Default for ControlHeader {
    fn default() -> Self {
        Self {
            ty: CommandTy::Undefined,
            flags: 0,
            fence_id: 0,
            ctx_id: 0,
            ring_index: 0,
            padding: [0; 3],
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct GpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl GpuRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct DisplayInfo {
    rect: GpuRect,
    pub enabled: u32,
    pub flags: u32,
}

#[derive(Debug)]
#[repr(C)]
pub struct GetDisplayInfo {
    pub header: ControlHeader,
    pub display_info: [DisplayInfo; VIRTIO_GPU_MAX_SCANOUTS],
}

impl Default for GetDisplayInfo {
    fn default() -> Self {
        Self {
            header: ControlHeader {
                ty: CommandTy::GetDisplayInfo,
                ..Default::default()
            },

            display_info: unsafe { core::mem::zeroed() },
        }
    }
}

static RESOURCE_ALLOC: AtomicU32 = AtomicU32::new(1); // XXX: 0 is reserved for whatever that takes `resource_id`.

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
#[repr(C)]
pub struct ResourceId(u32);

impl ResourceId {
    fn alloc() -> Self {
        ResourceId(RESOURCE_ALLOC.fetch_add(1, Ordering::SeqCst))
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
pub enum ResourceFormat {
    Unknown = 0,

    Bgrx = 2,
    Xrgb = 4,
}

#[derive(Debug)]
#[repr(C)]
pub struct ResourceCreate2d {
    pub header: ControlHeader,
    resource_id: ResourceId,
    format: ResourceFormat,
    width: u32,
    height: u32,
}

impl ResourceCreate2d {
    fn new(resource_id: ResourceId, format: ResourceFormat, width: u32, height: u32) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::ResourceCreate2d),
            resource_id,
            format,
            width,
            height,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct MemEntry {
    pub address: u64,
    pub length: u32,
    pub padding: u32,
}

#[derive(Debug)]
#[repr(C)]
pub struct AttachBacking {
    pub header: ControlHeader,
    pub resource_id: ResourceId,
    pub num_entries: u32,
}

impl AttachBacking {
    pub fn new(resource_id: ResourceId, num_entries: u32) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::ResourceAttachBacking),
            resource_id,
            num_entries,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct DetachBacking {
    pub header: ControlHeader,
    pub resource_id: ResourceId,
    pub padding: u32,
}

impl DetachBacking {
    pub fn new(resource_id: ResourceId) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::ResourceDetachBacking),
            resource_id,
            padding: 0,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct ResourceFlush {
    pub header: ControlHeader,
    pub rect: GpuRect,
    pub resource_id: ResourceId,
    pub padding: u32,
}

impl ResourceFlush {
    pub fn new(resource_id: ResourceId, rect: GpuRect) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::ResourceFlush),
            rect,
            resource_id,
            padding: 0,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct ResourceUnref {
    pub header: ControlHeader,
    pub resource_id: ResourceId,
    pub padding: u32,
}

impl ResourceUnref {
    pub fn new(resource_id: ResourceId) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::ResourceUnref),
            resource_id,
            padding: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct SetScanout {
    pub header: ControlHeader,
    pub rect: GpuRect,
    pub scanout_id: u32,
    pub resource_id: ResourceId,
}

impl SetScanout {
    pub fn new(scanout_id: u32, resource_id: ResourceId, rect: GpuRect) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::SetScanout),

            rect,
            scanout_id,
            resource_id,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct XferToHost2d {
    pub header: ControlHeader,
    pub rect: GpuRect,
    pub offset: u64,
    pub resource_id: ResourceId,
    pub padding: u32,
}

impl XferToHost2d {
    pub fn new(resource_id: ResourceId, rect: GpuRect, offset: u64) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::TransferToHost2d),
            rect,
            offset,
            resource_id,
            padding: 0,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct GetEdid {
    pub header: ControlHeader,
    pub scanout: u32,
    pub padding: u32,
}

impl GetEdid {
    pub fn new(scanout_id: u32) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::GetEdid),
            scanout: scanout_id,
            padding: 0,
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct GetEdidResp {
    pub header: ControlHeader,
    pub size: u32,
    pub padding: u32,
    pub edid: [u8; 1024],
}

impl GetEdidResp {
    pub fn new() -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::GetEdid),
            size: 0,
            padding: 0,
            edid: [0; 1024],
        }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct CursorPos {
    pub scanout_id: u32,
    pub x: i32,
    pub y: i32,
    _padding: u32,
}

impl CursorPos {
    pub fn new(scanout_id: u32, x: i32, y: i32) -> Self {
        Self {
            scanout_id,
            x,
            y,
            _padding: 0,
        }
    }
}

/* VIRTIO_GPU_CMD_UPDATE_CURSOR, VIRTIO_GPU_CMD_MOVE_CURSOR */
#[derive(Debug)]
#[repr(C)]
pub struct UpdateCursor {
    pub header: ControlHeader,
    pub pos: CursorPos,
    pub resource_id: ResourceId,
    pub hot_x: i32,
    pub hot_y: i32,
    _padding: u32,
}

impl UpdateCursor {
    pub fn update_cursor(x: i32, y: i32, hot_x: i32, hot_y: i32, resource_id: ResourceId) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::UpdateCursor),
            pos: CursorPos::new(0, x, y),
            resource_id,
            hot_x,
            hot_y,
            _padding: 0,
        }
    }
}

pub struct MoveCursor {
    pub header: ControlHeader,
    pub pos: CursorPos,
    pub resource_id: ResourceId,
    pub hot_x: i32,
    pub hot_y: i32,
    _padding: u32,
}

impl MoveCursor {
    pub fn move_cursor(x: i32, y: i32) -> Self {
        Self {
            header: ControlHeader::with_ty(CommandTy::MoveCursor),
            pos: CursorPos::new(0, x, y),
            resource_id: ResourceId(0),
            hot_x: 0,
            hot_y: 0,
            _padding: 0,
        }
    }
}

static DEVICE: spin::Once<virtio_core::Device> = spin::Once::new();

fn main() {
    pcid_interface::pci_daemon(daemon_runner);
}

fn daemon_runner(daemon: daemon::Daemon, pcid_handle: PciFunctionHandle) -> ! {
    deamon(daemon, pcid_handle).unwrap();
    unreachable!();
}

fn deamon(deamon: daemon::Daemon, mut pcid_handle: PciFunctionHandle) -> anyhow::Result<()> {
    eprintln!("[virtio-gpud] [1] daemon fn entered");
    common::setup_logging(
        "graphics",
        "pci",
        "virtio-gpud",
        common::output_level(),
        common::file_level(),
    );
    eprintln!("[virtio-gpud] [2] logging setup done");

    // Double check that we have the right device (0x1050 = virtio-gpu)
    let pci_config = pcid_handle.config();
    eprintln!("[virtio-gpud] [3] got pci config, device_id={:#x}", pci_config.func.full_device_id.device_id);
    assert_eq!(pci_config.func.full_device_id.device_id, 0x1050);
    eprintln!("[virtio-gpud] [4] device ID verified");
    log::info!("virtio-gpu: initiating startup sequence");

    eprintln!("[virtio-gpud] [5] calling probe_device");
    let device = DEVICE.try_call_once(|| virtio_core::probe_device(&mut pcid_handle))?;
    eprintln!("[virtio-gpud] [6] probe_device done");
    let config = unsafe { &mut *(device.device_space as *mut GpuConfig) };

    // Negotiate features (EDID disabled for now)
    let has_edid = false;
    device.transport.finalize_features();
    eprintln!("[virtio-gpud] [7] features finalized");

    // Queue for sending control commands
    eprintln!("[virtio-gpud] [8] setting up control queue");
    let control_queue = device
        .transport
        .setup_queue(MSIX_PRIMARY_VECTOR, &device.irq_handle)?;
    eprintln!("[virtio-gpud] [9] control queue done");

    // Queue for sending cursor updates
    eprintln!("[virtio-gpud] [10] setting up cursor queue");
    let cursor_queue = device
        .transport
        .setup_queue(MSIX_PRIMARY_VECTOR, &device.irq_handle)?;
    eprintln!("[virtio-gpud] [11] cursor queue done");

    device.transport.setup_config_notify(MSIX_PRIMARY_VECTOR);
    device.transport.run_device();
    eprintln!("[virtio-gpud] [12] device running");

    // Create the display scheme BEFORE signaling ready, so fbbootlogd/fbcond can find it
    eprintln!("[virtio-gpud] [13] creating GpuScheme");
    let (mut scheme, mut inputd_handle) = scheme::GpuScheme::new(
        config,
        control_queue.clone(),
        cursor_queue.clone(),
        device.transport.clone(),
        has_edid,
    )?;
    eprintln!("[virtio-gpud] [14] GpuScheme created");

    // Now signal that the daemon is ready (display scheme exists)
    eprintln!("[virtio-gpud] [15] calling daemon.ready()");
    deamon.ready();
    eprintln!("[virtio-gpud] [16] daemon.ready() done, entering event loop");

    user_data! {
        enum Source {
            Input,
            Scheme,
            Interrupt,
            Timer,
        }
    }

    let event_queue: EventQueue<Source> =
        EventQueue::new().expect("virtio-gpud: failed to create event queue");

    // Open a timer fd to ensure periodic polling (workaround for event notification issues)
    // The timer may not be available during early boot, so make it optional
    let timer_fd = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/scheme/time/10000000"); // 10ms timer

    if let Ok(ref fd) = timer_fd {
        eprintln!("[virtio-gpud] [16b] timer fd={}", fd.as_raw_fd());
        event_queue
            .subscribe(
                fd.as_raw_fd() as usize,
                Source::Timer,
                event::EventFlags::READ,
            )
            .unwrap();
    } else {
        eprintln!("[virtio-gpud] [16b] timer not available (early boot), using poll workaround");
    }

    // Register for EVENT_READ events from inputd - this tells inputd's scheme to notify us
    // fevent syscall: fd, event flags -> result with current event flags
    eprintln!("[virtio-gpud] [17a] calling fevent on inputd handle fd={}", inputd_handle.inner().as_raw_fd());
    let fevent_result = unsafe {
        syscall::syscall2(syscall::SYS_FEVENT, inputd_handle.inner().as_raw_fd() as usize, EventFlags::EVENT_READ.bits())
    };
    eprintln!("[virtio-gpud] [17b] fevent returned {:?}", fevent_result);
    fevent_result.expect("virtio-gpud: failed to register for inputd events");

    event_queue
        .subscribe(
            inputd_handle.inner().as_raw_fd() as usize,
            Source::Input,
            event::EventFlags::READ,
        )
        .unwrap();
    // Also register fevent for scheme socket explicitly
    let scheme_fd = scheme.event_handle().raw();
    eprintln!("[virtio-gpud] [17c] registering fevent for scheme socket fd={}", scheme_fd);
    let scheme_fevent_result = unsafe {
        syscall::syscall2(syscall::SYS_FEVENT, scheme_fd as usize, EventFlags::EVENT_READ.bits())
    };
    eprintln!("[virtio-gpud] [17d] scheme fevent returned {:?}", scheme_fevent_result);

    event_queue
        .subscribe(
            scheme_fd as usize,
            Source::Scheme,
            event::EventFlags::READ,
        )
        .unwrap();
    event_queue
        .subscribe(
            device.irq_handle.as_raw_fd() as usize,
            Source::Interrupt,
            event::EventFlags::READ,
        )
        .unwrap();

    let all = [Source::Input, Source::Scheme, Source::Interrupt];
    eprintln!("[virtio-gpud] [17] starting event loop iteration");
    let mut event_count = 0usize;
    let mut timer_buf = [0u8; 8];

    // Process initial events first
    for source in all {
        event_count += 1;
        eprintln!("[virtio-gpud] *** Initial event #{} ({:?})", event_count, source);
        match source {
            Source::Input => {
                eprintln!("[virtio-gpud] EVENT: Input");
                while let Some(vt_event) = inputd_handle
                    .read_vt_event()
                    .expect("virtio-gpud: failed to read display handle")
                {
                    eprintln!("[virtio-gpud] Got vt_event: {:?}", vt_event);
                    scheme.handle_vt_event(vt_event);
                }
            }
            Source::Scheme => {
                eprintln!("[virtio-gpud] EVENT: Scheme");
                scheme
                    .tick()
                    .expect("virtio-gpud: failed to process scheme events");
            }
            Source::Interrupt => {
                eprintln!("[virtio-gpud] EVENT: Interrupt");
                // Process interrupt inline
            }
            Source::Timer => {
                // Drain timer and poll all sources
            }
        }
    }

    eprintln!("[virtio-gpud] [18] Initial events done, entering main event loop");

    // Poll scheme once more before entering blocking loop to catch any events
    // that arrived between fevent registration and now
    eprintln!("[virtio-gpud] [18b] Pre-loop scheme poll");
    if let Err(e) = scheme.tick() {
        if e.kind() != std::io::ErrorKind::WouldBlock {
            eprintln!("[virtio-gpud] Pre-loop poll error: {:?}", e);
        }
    }

    // Use a simple polling loop instead of relying on event notifications
    // This is a workaround for event notification issues on aarch64
    eprintln!("[virtio-gpud] [18c] Starting polling-based event loop");
    let _ = std::fs::write("/scheme/debug/no-preserve", b"EQ\n"); // EQ = entering event queue

    loop {
        // Poll scheme for any pending requests
        let _ = scheme.tick();

        // Small sleep to avoid busy-waiting (1ms)
        std::thread::sleep(std::time::Duration::from_millis(1));

        // Also try to get an event (non-blocking check would be ideal, but iterator blocks)
        // For now, just poll the scheme continuously
    }

    // Original event queue code preserved but unreachable
    #[allow(unreachable_code)]
    for event_result in event_queue {
        let _ = std::fs::write("/scheme/debug/no-preserve", b"EV\n"); // EV = got event
        let event = event_result.expect("virtio-gpud: failed to get next event");
        let source: Source = event.user_data;
        event_count += 1;
        eprintln!("[virtio-gpud] *** Event #{} received ({:?})", event_count, source);
        match source {
            Source::Input => {
                eprintln!("[virtio-gpud] EVENT: Input");
                while let Some(vt_event) = inputd_handle
                    .read_vt_event()
                    .expect("virtio-gpud: failed to read display handle")
                {
                    eprintln!("[virtio-gpud] Got vt_event: {:?}", vt_event);
                    scheme.handle_vt_event(vt_event);
                }
            }
            Source::Scheme => {
                eprintln!("[virtio-gpud] EVENT: Scheme");
                scheme
                    .tick()
                    .expect("virtio-gpud: failed to process scheme events");
            }
            Source::Interrupt => {
                eprintln!("[virtio-gpud] EVENT: Interrupt");
                loop {
                // Read ISR to acknowledge the interrupt (required for legacy INTx on aarch64)
                let _isr_status = device.read_isr_status();

                let before_gen = device.transport.config_generation();

                let events = scheme.adapter().config.events_read.get();

                if events & VIRTIO_GPU_EVENT_DISPLAY != 0 {
                    let standard_properties = scheme.standard_properties();
                    let (adapter, objects) = scheme.adapter_and_objects_mut();
                    futures::executor::block_on(async { adapter.update_displays().await.unwrap() });
                    for connector_id in objects.connector_ids().to_vec() {
                        adapter.probe_connector(objects, &standard_properties, connector_id);
                    }
                    scheme.notify_displays_changed();
                    scheme
                        .adapter_mut()
                        .config
                        .events_clear
                        .set(VIRTIO_GPU_EVENT_DISPLAY);
                }

                let after_gen = device.transport.config_generation();
                if before_gen == after_gen {
                    break;
                }
                } // end loop
            }
            Source::Timer => {
                // Timer fired - drain it and poll scheme for any missed events
                use std::io::Read;
                if let Ok(ref fd) = timer_fd {
                    let _ = fd.try_clone().and_then(|mut f| f.read(&mut timer_buf));
                }
            }
        }

        // After processing any event, also poll scheme to catch any missed events
        // This is a workaround for potential event notification race conditions
        if let Err(e) = scheme.tick() {
            if e.kind() != std::io::ErrorKind::WouldBlock {
                eprintln!("[virtio-gpud] Post-event poll error: {:?}", e);
            }
        }
    }

    std::process::exit(0);
}
