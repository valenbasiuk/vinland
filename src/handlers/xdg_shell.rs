// xdg shell handler -> xdg_wm_base, xdg_surface, xdg_toplevel

use smithay::wayland::shell::xdg::{
    XdgShellHandler, XdgShellState,
    ToplevelSurface, PopupSurface, PositionerState,
};
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::Serial;

use smithay::reexports::wayland_server::Resource;
use tracing::info;
use crate::state::Vinland;

impl XdgShellHandler for Vinland {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // llamado cuando un cliente crea una nueva ventana
    // configure para que el cliente sepa que puede empezar
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.send_configure();
        info!("nueva ventana: {:?}", surface.wl_surface().id());
        self.windows.push(surface);
    }

    // llamado cuando una ventana toplevel es cerrada por el cliente
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.windows.retain(|w| w.wl_surface() != surface.wl_surface());
        info!("ventana cerrada por el cliente");
    }

    // popups: ventanas con padre (menús contextuales, dropdowns, etc.)
    fn new_popup(&mut self, _surface: PopupSurface, _positioner: PositionerState) {}
    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
    fn reposition_request(
        &mut self,
        _surface: PopupSurface,
        _positioner: PositionerState,
        _token: u32,
    ) {}
}
