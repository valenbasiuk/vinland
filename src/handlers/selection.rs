// selection handler -> wl_data_device_manager, wl_data_device, wl_data_source, etc.
// copy and paster

use smithay::wayland::selection::{
    SelectionHandler, SelectionTarget, SelectionSource,
};
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState,
};
use smithay::input::dnd::{Source, GrabType};
use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;

use crate::state::Vinland;

// SelectionHandler -> gestiona el clipboard a nivel de Wayland
impl SelectionHandler for Vinland {
    type SelectionUserData = ();

    // llamado cuando un cliente establece una nueva selección (copiar algo)
    fn new_selection(
        &mut self,
        _ty: SelectionTarget,
        _source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {}
}

// WaylandDndGrabHandler -> gestiona la negociación de drag-and-drop
impl smithay::wayland::selection::data_device::WaylandDndGrabHandler for Vinland {
    // llamado cuando un cliente inicia una operación drag-and-drop
    fn dnd_requested<S: Source>(
        &mut self,
        _source: S,
        _icon: Option<WlSurface>,
        _seat: Seat<Self>,
        _serial: Serial,
        _type: GrabType,
    ) {}
}

// DataDeviceHandler -> el entrypoint que vincula el gestor de dispositivos al compositor
impl DataDeviceHandler for Vinland {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}
