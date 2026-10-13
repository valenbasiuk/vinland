// handlers/text_input.rs
// soporte de IME y métodos de entrada via zwp_text_input_v3 y zwp_input_method_v2

use smithay::desktop::{PopupKind, PopupManager};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::input_method::{InputMethodHandler, PopupSurface};
use tracing::{info, warn};

use crate::state::Vinland;

impl InputMethodHandler for Vinland {
    fn new_popup(&mut self, surface: PopupSurface) {
        info!("[IME] nuevo popup de método de entrada registrado");
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            warn!("[IME] error al registrar popup de método de entrada: {}", err);
        }
    }

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        info!("[IME] cerrando popup de método de entrada");
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {
        // el posicionamiento es gestionado internamente por smithay
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        // buscar en ventanas de clientes normales
        if let Some(win) = self.windows().iter().find(|w| w.surface.wl_surface() == parent) {
            return win.rect;
        }

        // fallback
        Rectangle::default()
    }
}
