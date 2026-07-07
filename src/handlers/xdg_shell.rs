// xdg shell handler -> xdg_wm_base, xdg_surface, xdg_toplevel
// extension de surface q da  propiedades de titulos, estados y arrastre

use smithay::wayland::shell::xdg::{
    XdgShellHandler, XdgShellState,
    ToplevelSurface, PopupSurface, PositionerState,
};
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::{Serial, SERIAL_COUNTER, Rectangle};

use smithay::reexports::wayland_server::Resource;
use tracing::info;
use crate::state::{Vinland, Window};

impl XdgShellHandler for Vinland {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    // llamado cuando un cliente crea una nueva ventana toplevel
    // agregamos con rect (0,0,0,0) de placeholder y luego retile() le asigna el rect real
    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        info!("nueva ventana: {:?}", surface.wl_surface().id());

        // rect placeholder hasta que retile() calcule el tamaño real
        self.windows.push(Window {
            surface,
            rect: Rectangle::new((0, 0).into(), (0, 0).into()),
        });

        // retile() asigna posición y tamaño a todas las ventanas y envía configure
        self.retile();

        // foco de teclado a la ventana recien abierta
        let serial = SERIAL_COUNTER.next_serial();
        let wl_surface = self.windows.last().unwrap().surface.wl_surface().clone();
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.set_focus(self, Some(wl_surface), serial);
    }

    // llamado cuando una ventana toplevel es cerrada por el cliente
    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.windows.retain(|w| w.surface.wl_surface() != surface.wl_surface());
        info!("ventana cerrada por el cliente");
        // retile() redistribuye el espacio entre las ventanas restantes
        self.retile();
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
