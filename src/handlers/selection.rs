// selection handler -> wl_data_device_manager, wl_data_device, wl_data_source, etc.
// gestiona clipboard y drag-and-drop entre aplicaciones

use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::Focus;
use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::Serial;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler,
};
use smithay::wayland::selection::{
    SelectionHandler, SelectionSource, SelectionTarget,
};

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

// WaylandDndGrabHandler -> gestiona la negociación e inicio del drag-and-drop de Wayland
impl WaylandDndGrabHandler for Vinland {
    // llamado cuando un cliente inicia una operación drag-and-drop
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        ty: GrabType,
    ) {
        tracing::info!("[dnd] drag-and-drop iniciado: icon={:?}, type={:?}", icon.as_ref().map(|s| s.id()), ty);
        self.dnd_icon = icon;
        match ty {
            GrabType::Pointer => {
                let pointer = match seat.get_pointer() {
                    Some(p) => p,
                    None => return,
                };
                let start_data = match pointer.grab_start_data() {
                    Some(d) => d,
                    None => return,
                };
                pointer.set_grab(
                    self,
                    DnDGrab::new_pointer(&self.display_handle, start_data, source, seat),
                    serial,
                    Focus::Keep,
                );
            }
            GrabType::Touch => {
                let touch = match seat.get_touch() {
                    Some(t) => t,
                    None => return,
                };
                let start_data = match touch.grab_start_data() {
                    Some(d) => d,
                    None => return,
                };
                touch.set_grab(
                    self,
                    DnDGrab::new_touch(&self.display_handle, start_data, source, seat),
                    serial,
                );
            }
        }
    }
}

// DndGrabHandler -> limpia el estado cuando el drag-and-drop finaliza o se suelta el item
impl DndGrabHandler for Vinland {
    fn dropped(
        &mut self,
        _target: Option<smithay::input::dnd::DndTarget<'_, Self>>,
        _validated: bool,
        _seat: Seat<Self>,
        _location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        tracing::info!("[dnd] drag-and-drop finalizado (dropped)");
        self.dnd_icon = None;
        self.backend.window().request_redraw();
    }
}

// DataDeviceHandler -> el entrypoint que vincula el gestor de dispositivos al compositor
impl DataDeviceHandler for Vinland {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}
