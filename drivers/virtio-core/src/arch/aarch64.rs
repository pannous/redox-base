use std::fs::File;

use pcid_interface::*;

use crate::transport::Error;

pub fn enable_msix(pcid_handle: &mut PciFunctionHandle) -> Result<File, Error> {
    // MSI-X on aarch64 requires GICv3 ITS which isn't fully supported yet.
    // Fall back to legacy INTx# pin-based interrupts.
    if let Some(irq) = pcid_handle.config().func.legacy_interrupt_line {
        log::debug!("virtio: aarch64 using legacy INTx# interrupt (MSI-X not yet supported)");
        return Ok(irq.irq_handle("virtio"));
    }

    log::error!("virtio: aarch64 no legacy interrupt available and MSI-X not supported");
    Err(Error::InCapable(crate::spec::CfgType::Common))
}
