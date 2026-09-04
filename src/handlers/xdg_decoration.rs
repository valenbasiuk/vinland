// xdg decoration handler -> negociacion de decoraciones server-side vs client-side
// el modo se controla desde config.decoration.mode:
//   ServerSide -> siempre SSD, el compositor dibuja bordes/titlebar
//   ClientSide -> siempre CSD, las apps dibujan sus propias decoraciones
//   Auto       -> respetar la preferencia de la app; ServerSide como fallback

use crate::config::DecorationMode;
use crate::state::Vinland;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::{decoration::XdgDecorationHandler, ToplevelSurface};

impl XdgDecorationHandler for Vinland {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        let mode = self.effective_decoration_mode(None);
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        let effective = self.effective_decoration_mode(Some(mode));
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(effective);
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        // sin preferencia de la app -> usar el modo configurado (ServerSide por defecto)
        let mode = self.effective_decoration_mode(None);
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
        });
        toplevel.send_configure();
    }
}

impl Vinland {
    /// Devuelve el Mode wayland a aplicar segun config y la preferencia de la app.
    /// app_request = None si la app no expreso preferencia (new_decoration / unset_mode)
    pub fn effective_decoration_mode(&self, app_request: Option<Mode>) -> Mode {
        match self.config.decoration.mode {
            // config fuerza SSD para todos sin excepcion
            DecorationMode::ServerSide => Mode::ServerSide,
            // config fuerza CSD para todos sin excepcion
            DecorationMode::ClientSide => Mode::ClientSide,
            // Auto: honrar la preferencia de la app; ServerSide si no pide nada
            DecorationMode::Auto => match app_request {
                Some(Mode::ClientSide) => Mode::ClientSide,
                _ => Mode::ServerSide,
            },
        }
    }
}
