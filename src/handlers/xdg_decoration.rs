// xdg decoration handler -> negociacion de decoraciones server-side vs client-side
// permite avisar a clientes como GTK/gedit y Qt/kate que el compositor provee decoraciones SSD
// eliminando sombras y márgenes CSD residuales

use crate::state::Vinland;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::{decoration::XdgDecorationHandler, ToplevelSurface};

impl XdgDecorationHandler for Vinland {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        toplevel.with_pending_state(|state| {
            // siempre configuramos ServerSide para unificar el tiling y evitar sombras CSD
            let _ = mode;
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_configure();
    }
}
